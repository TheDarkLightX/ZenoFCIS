#![no_std]
#![no_main]

extern crate alloc;

use alloc::vec;
use bootloader_api::info::{FrameBufferInfo, PixelFormat};
use bootloader_api::{BootInfo, entry_point};
use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::fmt::{self, Write as _};
use core::panic::PanicInfo;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use zeno_fcis_spec::{
    MiniBudget, MiniCommand, MiniDecision, MiniDeterminator, MiniState, PrivateWork,
    WorkerInstruction, WorkerProgram, WorkspaceCell,
};

const HEAP_BYTES: usize = 8 * 1024 * 1024;

#[repr(align(4096))]
struct HeapCell(UnsafeCell<[u8; HEAP_BYTES]>);

// SAFETY: the bump allocator below grants each byte range exactly once. The
// kernel is single-core and never exposes the backing cell directly.
unsafe impl Sync for HeapCell {}

static HEAP: HeapCell = HeapCell(UnsafeCell::new([0; HEAP_BYTES]));
static NEXT_ALLOCATION: AtomicUsize = AtomicUsize::new(0);

struct KernelAllocator;

// SAFETY: allocation uses an atomic monotonic cursor, checked alignment, and a
// fixed exclusively owned heap. Deallocation is intentionally a no-op because
// this bounded demo performs one transition and then halts.
unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align_mask = layout.align().saturating_sub(1);
        let mut cursor = NEXT_ALLOCATION.load(Ordering::Relaxed);
        loop {
            let Some(aligned) = cursor
                .checked_add(align_mask)
                .map(|value| value & !align_mask)
            else {
                return ptr::null_mut();
            };
            let Some(end) = aligned.checked_add(layout.size()) else {
                return ptr::null_mut();
            };
            if end > HEAP_BYTES {
                return ptr::null_mut();
            }
            match NEXT_ALLOCATION.compare_exchange_weak(
                cursor,
                end,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // SAFETY: `aligned..end` was reserved uniquely above and is
                    // within the fixed heap allocation.
                    return unsafe { HEAP.0.get().cast::<u8>().add(aligned) };
                }
                Err(next) => cursor = next,
            }
        }
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOCATOR: KernelAllocator = KernelAllocator;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial_init();
    serial_line(format_args!("ZENOFCIS_QEMU_DEMO/1"));
    serial_line(format_args!("BOOT=KERNEL"));
    serial_line(format_args!("FIRMWARE_HANDOFF=COMPLETE"));
    serial_line(format_args!("TARGET=x86_64-unknown-none"));
    serial_line(format_args!("CORE=zeno-fcis-spec/1.0.0-rc.3"));

    let outcome = run_semantic_demo();
    serial_line(format_args!("REPLAY_ORDER_A=2,1"));
    serial_line(format_args!("REPLAY_ORDER_B=1,2"));
    serial_line(format_args!("REPLAY=PASS"));
    for cell in outcome.cells {
        serial_line(format_args!("SLOT_{}={}", cell.slot(), cell.value()));
    }
    for (worker, value) in outcome.returns {
        serial_line(format_args!("WORKER_{}_RETURN={}", worker, value));
    }
    serial_line(format_args!(
        "CONFLICT=SLOT_{}:WORKERS_{}_{}",
        outcome.conflict_slot, outcome.conflict_workers.0, outcome.conflict_workers.1
    ));
    serial_line(format_args!("AUTHORITY_CHANGE=NONE"));

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        let info = framebuffer.info();
        let mut canvas = Canvas::new(framebuffer.buffer_mut(), info);
        draw_demo(&mut canvas);
        serial_line(format_args!("FRAMEBUFFER={}x{}", info.width, info.height));
    } else {
        serial_line(format_args!("FRAMEBUFFER=UNAVAILABLE"));
    }

    serial_line(format_args!("QEMU_DEMO_COMPLETE"));
    halt()
}

struct DemoOutcome {
    cells: [WorkspaceCell; 3],
    returns: [(u32, i128); 2],
    conflict_slot: u32,
    conflict_workers: (u32, u32),
}

fn worker(id: u32, output: u32, operation: WorkerInstruction) -> WorkerProgram {
    WorkerProgram::new(
        id,
        vec![1],
        vec![output],
        vec![
            WorkerInstruction::Get(1),
            operation,
            WorkerInstruction::Put(output),
            WorkerInstruction::Return,
        ],
    )
}

fn run_semantic_demo() -> DemoOutcome {
    let pre = MiniState::try_new(vec![WorkspaceCell::new(1, 10)])
        .unwrap_or_else(|| panic!("static pre-state must be valid"));
    let programs = vec![
        worker(2, 3, WorkerInstruction::Multiply(2)),
        worker(1, 2, WorkerInstruction::Add(5)),
    ];
    let order_a =
        MiniDeterminator::execute_programs(&pre, &programs, &[2, 1], MiniBudget::default());
    let order_b =
        MiniDeterminator::execute_programs(&pre, &programs, &[1, 2], MiniBudget::default());
    assert_eq!(order_a, order_b, "completion order changed the result");

    let (cells, returns) = match order_a.decision() {
        MiniDecision::Accepted { state, returns } => {
            let [first, second, third] = state.cells() else {
                panic!("accepted demo state must contain three cells");
            };
            let [first_return, second_return] = returns.as_ref() else {
                panic!("accepted demo must contain two returns");
            };
            ([*first, *second, *third], [*first_return, *second_return])
        }
        MiniDecision::Rejected(_) => panic!("disjoint worker programs conflicted"),
        MiniDecision::Blocked(_) => panic!("bounded worker programs were blocked"),
    };

    let pre_before_conflict = pre.clone();
    let conflict = MiniDeterminator::execute(
        &pre,
        &MiniCommand::Execute(vec![
            PrivateWork::new(2, vec![4], vec![WorkspaceCell::new(4, 22)], 22),
            PrivateWork::new(1, vec![4], vec![WorkspaceCell::new(4, 11)], 11),
        ]),
        MiniBudget::default(),
    );
    assert_eq!(pre, pre_before_conflict, "conflict mutated pre-state");
    let MiniDecision::Rejected(witness) = conflict else {
        panic!("conflicting writes did not reject");
    };

    DemoOutcome {
        cells,
        returns,
        conflict_slot: witness.slot(),
        conflict_workers: (witness.first_worker(), witness.second_worker()),
    }
}

#[derive(Clone, Copy)]
struct Color {
    red: u8,
    green: u8,
    blue: u8,
}

const BACKGROUND: Color = Color {
    red: 5,
    green: 9,
    blue: 18,
};
const PANEL: Color = Color {
    red: 12,
    green: 24,
    blue: 39,
};
const BORDER: Color = Color {
    red: 38,
    green: 63,
    blue: 82,
};
const ACCENT: Color = Color {
    red: 94,
    green: 234,
    blue: 212,
};
const TEXT: Color = Color {
    red: 236,
    green: 247,
    blue: 255,
};
const MUTED: Color = Color {
    red: 145,
    green: 166,
    blue: 190,
};

struct Canvas<'a> {
    buffer: &'a mut [u8],
    info: FrameBufferInfo,
}

impl<'a> Canvas<'a> {
    fn new(buffer: &'a mut [u8], info: FrameBufferInfo) -> Self {
        Self { buffer, info }
    }

    fn clear(&mut self, color: Color) {
        for y in 0..self.info.height {
            for x in 0..self.info.width {
                self.pixel(x, y, color);
            }
        }
    }

    fn pixel(&mut self, x: usize, y: usize, color: Color) {
        if x >= self.info.width || y >= self.info.height {
            return;
        }
        let offset = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        if offset + self.info.bytes_per_pixel > self.buffer.len() {
            return;
        }
        let bytes = match self.info.pixel_format {
            PixelFormat::Rgb => [color.red, color.green, color.blue, 0],
            PixelFormat::Bgr => [color.blue, color.green, color.red, 0],
            PixelFormat::U8 => {
                let gray = ((u16::from(color.red) + u16::from(color.green) + u16::from(color.blue))
                    / 3) as u8;
                [gray, gray, gray, 0]
            }
            _ => [color.blue, color.green, color.red, 0],
        };
        let count = self.info.bytes_per_pixel.min(bytes.len());
        self.buffer[offset..offset + count].copy_from_slice(&bytes[..count]);
    }

    fn rectangle(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        let max_y = y.saturating_add(height).min(self.info.height);
        let max_x = x.saturating_add(width).min(self.info.width);
        for py in y..max_y {
            for px in x..max_x {
                self.pixel(px, py, color);
            }
        }
    }

    fn outline(&mut self, x: usize, y: usize, width: usize, height: usize, color: Color) {
        self.rectangle(x, y, width, 2, color);
        self.rectangle(
            x,
            y.saturating_add(height.saturating_sub(2)),
            width,
            2,
            color,
        );
        self.rectangle(x, y, 2, height, color);
        self.rectangle(
            x.saturating_add(width.saturating_sub(2)),
            y,
            2,
            height,
            color,
        );
    }

    fn text(&mut self, x: usize, y: usize, scale: usize, value: &str, color: Color) {
        let mut cursor = x;
        for byte in value.bytes() {
            self.glyph(cursor, y, scale, glyph(byte), color);
            cursor = cursor.saturating_add(6 * scale);
        }
    }

    fn glyph(&mut self, x: usize, y: usize, scale: usize, rows: [u8; 7], color: Color) {
        for (row, bits) in rows.into_iter().enumerate() {
            for column in 0..5 {
                if bits & (1 << (4 - column)) != 0 {
                    self.rectangle(x + column * scale, y + row * scale, scale, scale, color);
                }
            }
        }
    }
}

fn draw_demo(canvas: &mut Canvas<'_>) {
    canvas.clear(BACKGROUND);
    let width = canvas.info.width;
    let height = canvas.info.height;
    let margin = width / 16;
    canvas.rectangle(0, 0, 8, height, ACCENT);
    canvas.text(margin, height / 14, 4, "ZENOFCIS", TEXT);
    canvas.text(
        margin,
        height / 14 + 42,
        2,
        "MINI DETERMINATOR KERNEL",
        ACCENT,
    );
    canvas.text(
        margin,
        height / 14 + 72,
        2,
        "BOOTED IN QEMU / X86_64 / NO STD",
        MUTED,
    );

    let panel_y = height / 4;
    let panel_height = height * 3 / 5;
    canvas.rectangle(margin, panel_y, width - 2 * margin, panel_height, PANEL);
    canvas.outline(margin, panel_y, width - 2 * margin, panel_height, BORDER);
    canvas.text(
        margin + 28,
        panel_y + 30,
        2,
        "EXECUTABLE SEMANTIC RESULT",
        ACCENT,
    );
    canvas.text(
        margin + 28,
        panel_y + 70,
        3,
        "PRIVATE WORK / CANONICAL MERGE",
        TEXT,
    );

    let box_y = panel_y + 140;
    let available = width - 2 * margin - 80;
    let box_width = available / 3;
    for index in 0..3 {
        let x = margin + 28 + index * (box_width + 12);
        canvas.outline(x, box_y, box_width, 105, BORDER);
    }
    canvas.text(margin + 48, box_y + 18, 2, "SLOT 1", ACCENT);
    canvas.text(margin + 48, box_y + 54, 3, "10", TEXT);
    canvas.text(margin + 60 + box_width, box_y + 18, 2, "SLOT 2", ACCENT);
    canvas.text(margin + 60 + box_width, box_y + 54, 3, "15", TEXT);
    canvas.text(margin + 72 + box_width * 2, box_y + 18, 2, "SLOT 3", ACCENT);
    canvas.text(margin + 72 + box_width * 2, box_y + 54, 3, "20", TEXT);

    let status_y = box_y + 140;
    canvas.text(margin + 28, status_y, 2, "REPLAY ORDER 2 1 / 1 2", MUTED);
    canvas.text(margin + 28, status_y + 35, 3, "REPLAY PASS", ACCENT);
    canvas.text(
        margin + 28,
        status_y + 84,
        2,
        "CONFLICT SLOT 4 / WORKERS 1 2",
        MUTED,
    );
    canvas.text(
        margin + 28,
        status_y + 119,
        3,
        "REJECTED / AUTHORITY UNCHANGED",
        TEXT,
    );
}

fn glyph(byte: u8) -> [u8; 7] {
    match byte.to_ascii_uppercase() {
        b'A' => [0x0e, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        b'B' => [0x1e, 0x11, 0x11, 0x1e, 0x11, 0x11, 0x1e],
        b'C' => [0x0e, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0e],
        b'D' => [0x1e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1e],
        b'E' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x1f],
        b'F' => [0x1f, 0x10, 0x10, 0x1e, 0x10, 0x10, 0x10],
        b'G' => [0x0e, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0f],
        b'H' => [0x11, 0x11, 0x11, 0x1f, 0x11, 0x11, 0x11],
        b'I' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x1f],
        b'J' => [0x07, 0x02, 0x02, 0x02, 0x12, 0x12, 0x0c],
        b'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        b'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1f],
        b'M' => [0x11, 0x1b, 0x15, 0x15, 0x11, 0x11, 0x11],
        b'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        b'O' => [0x0e, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        b'P' => [0x1e, 0x11, 0x11, 0x1e, 0x10, 0x10, 0x10],
        b'Q' => [0x0e, 0x11, 0x11, 0x11, 0x15, 0x12, 0x0d],
        b'R' => [0x1e, 0x11, 0x11, 0x1e, 0x14, 0x12, 0x11],
        b'S' => [0x0f, 0x10, 0x10, 0x0e, 0x01, 0x01, 0x1e],
        b'T' => [0x1f, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        b'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0e],
        b'V' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x0a, 0x04],
        b'W' => [0x11, 0x11, 0x11, 0x15, 0x15, 0x15, 0x0a],
        b'X' => [0x11, 0x11, 0x0a, 0x04, 0x0a, 0x11, 0x11],
        b'Y' => [0x11, 0x11, 0x0a, 0x04, 0x04, 0x04, 0x04],
        b'Z' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x10, 0x1f],
        b'0' => [0x0e, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0e],
        b'1' => [0x04, 0x0c, 0x14, 0x04, 0x04, 0x04, 0x1f],
        b'2' => [0x0e, 0x11, 0x01, 0x02, 0x04, 0x08, 0x1f],
        b'3' => [0x1e, 0x01, 0x01, 0x0e, 0x01, 0x01, 0x1e],
        b'4' => [0x02, 0x06, 0x0a, 0x12, 0x1f, 0x02, 0x02],
        b'5' => [0x1f, 0x10, 0x10, 0x1e, 0x01, 0x01, 0x1e],
        b'6' => [0x0e, 0x10, 0x10, 0x1e, 0x11, 0x11, 0x0e],
        b'7' => [0x1f, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        b'8' => [0x0e, 0x11, 0x11, 0x0e, 0x11, 0x11, 0x0e],
        b'9' => [0x0e, 0x11, 0x11, 0x0f, 0x01, 0x01, 0x0e],
        b'/' => [0x01, 0x02, 0x04, 0x08, 0x10, 0x00, 0x00],
        b'-' => [0x00, 0x00, 0x00, 0x1f, 0x00, 0x00, 0x00],
        b'=' => [0x00, 0x1f, 0x00, 0x1f, 0x00, 0x00, 0x00],
        b':' => [0x00, 0x04, 0x04, 0x00, 0x04, 0x04, 0x00],
        _ => [0; 7],
    }
}

struct SerialPort;

impl SerialPort {
    fn write_byte(&mut self, byte: u8) {
        while unsafe { input_byte(0x3fd) } & 0x20 == 0 {}
        unsafe { output_byte(0x3f8, byte) };
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, value: &str) -> fmt::Result {
        for byte in value.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}

fn serial_init() {
    unsafe {
        output_byte(0x3f9, 0x00);
        output_byte(0x3fb, 0x80);
        output_byte(0x3f8, 0x03);
        output_byte(0x3f9, 0x00);
        output_byte(0x3fb, 0x03);
        output_byte(0x3fa, 0xc7);
        output_byte(0x3fc, 0x0b);
    }
}

fn serial_line(arguments: fmt::Arguments<'_>) {
    let mut serial = SerialPort;
    let _ = serial.write_fmt(arguments);
    let _ = serial.write_str("\n");
}

unsafe fn output_byte(port: u16, value: u8) {
    unsafe {
        core::arch::asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        )
    }
}

unsafe fn input_byte(port: u16) -> u8 {
    let value: u8;
    unsafe {
        core::arch::asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        )
    }
    value
}

#[panic_handler]
fn panic(info: &PanicInfo<'_>) -> ! {
    serial_line(format_args!("KERNEL_PANIC={info}"));
    halt()
}

fn halt() -> ! {
    loop {
        core::hint::spin_loop();
    }
}
