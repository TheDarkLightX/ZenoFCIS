import { spawn } from "node:child_process";
import { once } from "node:events";
import fs from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { painter } from "./draw.mjs";
import { poster } from "./poster.mjs";

const [dir, stem] = process.argv.slice(2);
const plan = JSON.parse(fs.readFileSync(path.join(dir, stem + ".plan.json"), "utf8"));
const { createCanvas, GlobalFonts } = createRequire(import.meta.url)("@napi-rs/canvas");
const fonts = {
  Face: [
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "C:/Windows/Fonts/arial.ttf",
  ],
  Heavy: [
    "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
    "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
    "C:/Windows/Fonts/arialbd.ttf",
  ],
  Code: [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/System/Library/Fonts/Monaco.ttf",
    "C:/Windows/Fonts/consola.ttf",
  ],
};
for (const [family, options] of Object.entries(fonts)) {
  const file = options.find((p) => fs.existsSync(p));
  if (file) GlobalFonts.registerFromPath(file, family);
}

async function render(theme) {
  const canvas = createCanvas(1600, 900);
  const draw = painter(canvas.getContext("2d"), plan, theme);
  const video = path.join(dir, stem + "-" + theme + ".mp4");
  const ff = spawn(
    "ffmpeg",
    [
      "-v",
      "error",
      "-n",
      "-f",
      "rawvideo",
      "-pix_fmt",
      "rgba",
      "-s",
      "1600x900",
      "-r",
      String(plan.fps),
      "-i",
      "pipe:0",
      "-an",
      "-c:v",
      "libx264",
      "-threads",
      "3",
      "-crf",
      "18",
      "-preset",
      "veryfast",
      "-pix_fmt",
      "yuv420p",
      "-movflags",
      "+faststart",
      video,
    ],
    { stdio: ["pipe", "ignore", "inherit"] },
  );
  const closed = once(ff, "close");
  // Also consume early pipe/process failures while a frame is being painted.
  closed.catch(() => {});
  ff.stdin.on("error", () => {});
  try {
    await once(ff, "spawn");
    for (const shot of plan.shots) {
      const frames = shot.endFrame - shot.startFrame;
      for (let frame = 0; frame < frames; frame++) {
        draw(shot, frame / (frames - 1));
        // Wait for the write before changing the backing pixels.
        await new Promise((resolve, reject) =>
          ff.stdin.write(canvas.data(), (error) => (error ? reject(error) : resolve())),
        );
      }
    }
    ff.stdin.end();
    const [status] = await closed;
    if (status !== 0) throw Error("FFmpeg failed with exit " + status);
  } finally {
    if (ff.exitCode === null && ff.signalCode === null) ff.kill();
  }
}

for (const theme of ["light", "dark"]) await render(theme);
poster(plan, path.join(dir, stem + ".svg"));
const stamp = (frame) => new Date(Math.round((frame * 1000) / plan.fps)).toISOString().slice(11, 23);
const cueText = (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
const cues = plan.shots.map(
  (s) =>
    stamp(s.startFrame) + " --> " + stamp(s.endFrame) + "\n" + cueText(s.title) + "\n" + cueText(s.caption),
);
fs.writeFileSync(path.join(dir, stem + ".vtt"), "WEBVTT\n\n" + cues.join("\n\n") + "\n");
