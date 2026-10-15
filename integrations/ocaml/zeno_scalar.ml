type t = { native : string; version : string }
type frame = string
type scalar = Int of int64 | Bool of bool | Variant of string
type report = { exit : int; stdout : string; stderr : string }
type error = Native of report | Refused of string | Unknown of string
let profile = "zeno-fcis/native-scalar-consumer/1"
let schema = "zeno-fcis/native-scalar-frame/1"
let max_frame_bytes = 65536
let max_output_bytes = 32 * 1024 * 1024
let seconds = 30.
let starts s prefix = String.starts_with ~prefix s
let ends s suffix = String.ends_with ~suffix s
let version_suffix =
  ",\"host_abi\":\"" ^ schema ^ "\",\"host_profile\":\"" ^ profile ^ "\"}\n"
exception Transport of string
let close fd = try Unix.close fd with Unix.Unix_error _ -> ()
let rec reap pid =
  try snd (Unix.waitpid [] pid)
  with Unix.Unix_error (Unix.EINTR, _, _) -> reap pid

(* No shell; concurrent pipe draining; one deadline shared by handshake/operation. *)
let process ~deadline native arguments =
  let descriptors = ref [] in
  let child = ref None in
  let status = ref None in
  let own fd = descriptors := fd :: !descriptors; fd in
  let cleanup () = List.iter close !descriptors; descriptors := [] in
  let stop () = match !child, !status with
    | Some pid, None ->
        (try Unix.kill pid Sys.sigkill with Unix.Unix_error _ -> ());
        ignore (reap pid)
    | _ -> () in
  try
    let output_r, output_w = Unix.pipe ~cloexec:true () in
    let output_r = own output_r and output_w = own output_w in
    let error_r, error_w = Unix.pipe ~cloexec:true () in
    let error_r = own error_r and error_w = own error_w in
    let null = own (Unix.openfile "/dev/null" [Unix.O_RDONLY; Unix.O_CLOEXEC] 0) in
    let pid = Unix.create_process native
      (Array.of_list (native :: arguments)) null output_w error_w in
    child := Some pid;
    List.iter close [null; output_w; error_w];
    descriptors := [output_r; error_r];
    Unix.set_nonblock output_r; Unix.set_nonblock error_r;
    let stdout = Buffer.create 256 and stderr = Buffer.create 256 in
    let open_pipes = ref [output_r; error_r] in
    let bytes = Bytes.create 4096 in
    while !open_pipes <> [] || !status = None do
      if !status = None then begin
        let found, value = Unix.waitpid [Unix.WNOHANG] pid in
        if found <> 0 then status := Some value
      end;
      let remaining = deadline -. Unix.gettimeofday () in
      if remaining <= 0. then raise (Transport "native operation deadline exceeded");
      let ready =
        try let ready, _, _ = Unix.select !open_pipes [] [] (min remaining 0.05) in ready
        with Unix.Unix_error (Unix.EINTR, _, _) -> [] in
      List.iter (fun fd ->
        try
          let count = Unix.read fd bytes 0 (Bytes.length bytes) in
          if count = 0 then begin
            close fd;
            open_pipes := List.filter ((<>) fd) !open_pipes;
            descriptors := List.filter ((<>) fd) !descriptors
          end else begin
            let buffer = if fd = output_r then stdout else stderr in
            if Buffer.length buffer + count > max_output_bytes then
              raise (Transport "native output cap exceeded");
            Buffer.add_subbytes buffer bytes 0 count
          end
        with Unix.Unix_error ((Unix.EAGAIN | Unix.EWOULDBLOCK | Unix.EINTR), _, _) -> ()
      ) ready
    done;
    cleanup ();
    match !status with
    | Some (Unix.WEXITED exit) when List.mem exit [0; 1; 2; 3; 64] ->
        Ok { exit; stdout = Buffer.contents stdout; stderr = Buffer.contents stderr }
    | _ -> Error (Unknown "native process ended without a declared exit status")
  with
  | Transport message -> stop (); cleanup (); Error (Unknown message)
  | (Unix.Unix_error _ | Invalid_argument _) as exception_ ->
      let started = !child <> None in
      stop (); cleanup ();
      let message = Printexc.to_string exception_ in
      Error (if started then Unknown message else Refused message)

let binding ~deadline t =
  match process ~deadline t.native ["version"; "--format"; "json"] with
  | Ok report when report.exit = 0 && report.stderr = "" && report.stdout = t.version -> Ok ()
  | Ok _ -> Error (Refused "installed native version/profile/identity mismatch")
  | Error error -> Error error

let create ~native ~version =
  if Filename.is_relative native then Error (Refused "native executable path must be absolute")
  else if String.length version > max_output_bytes || not (starts version "{\"status\":\"version\",")
          || not (ends version version_suffix) then
    Error (Refused "version binding does not declare the exact supported scalar profile")
  else
    let t = { native; version } in
    match binding ~deadline:(Unix.gettimeofday () +. seconds) t with
    | Ok () -> Ok t
    | Error error -> Error error

let invoke t arguments =
  let deadline = Unix.gettimeofday () +. seconds in
  match binding ~deadline t with
  | Error error -> Error error
  | Ok () -> process ~deadline t.native arguments

let frame_of_bytes bytes =
  if Bytes.length bytes > max_frame_bytes then Error (Refused "frame exceeds 65536 bytes")
  else Ok (Bytes.to_string bytes)
let frame_bytes frame = Bytes.of_string frame

let frame_of_report report =
  let prefix = "{\"schema\":\"" ^ schema ^ "\",\"profile\":\"" ^ profile ^ "\",\"frame\":\"" in
  let suffix = "\"}\n" in
  let count = String.length report - String.length prefix - String.length suffix in
  if count < 0 || count > 2 * max_frame_bytes || count mod 2 <> 0
     || not (starts report prefix) || not (ends report suffix) then
    Error (Refused "noncanonical native frame report")
  else
    let nibble character = match character with
      | '0' .. '9' -> Char.code character - Char.code '0'
      | 'a' .. 'f' -> Char.code character - Char.code 'a' + 10
      | _ -> raise (Transport "native frame is not lowercase hexadecimal") in
    try
      let bytes = Bytes.init (count / 2) (fun index ->
        let offset = String.length prefix + 2 * index in
        Char.chr ((nibble report.[offset] lsl 4) lor nibble report.[offset + 1])) in
      frame_of_bytes bytes
    with Transport message -> Error (Refused message)

let safe_name name =
  starts name "command." || starts name "context."
let safe_token token =
  String.length token > 0 && String.length token <= 128
  && String.for_all (function
       | 'a' .. 'z' | 'A' .. 'Z' | '0' .. '9' | '_' | '.' -> true | _ -> false) token
let frame t fields =
  if List.length fields > 16 || List.exists (fun (name, value) ->
      not (safe_name name && safe_token name)
      || match value with Variant name -> not (safe_token name) | _ -> false) fields then
    Error (Refused "unsupported scalar field name/count/variant spelling")
  else
    let words = List.map (fun (name, value) ->
      name ^ "=" ^ (match value with
        | Int number -> Int64.to_string number
        | Bool boolean -> string_of_bool boolean
        | Variant name -> name)) fields in
    match invoke t ("frame" :: words @ ["--format"; "json"]) with
    | Error error -> Error error
    | Ok report when report.exit <> 0 -> Error (Native report)
    | Ok report when report.stderr <> "" -> Error (Refused "native frame reported unexpected stderr")
    | Ok report -> frame_of_report report.stdout

let read t command database = invoke t [command; database; "--format"; "json"]
let init t database = read t "init" database
let state t database = read t "state" database
let history t database = read t "history" database
let with_frame t command database frame =
  let temporary = ref None in
  try
    let path, output = Filename.open_temp_file ~mode:[Open_binary] "zfcis-ocaml-" ".frame" in
    temporary := Some path;
    (try output_string output frame; close_out output
     with exception_ -> close_out_noerr output; raise exception_);
    let result = invoke t [command; database; "--frame"; path; "--format"; "json"] in
    (try Sys.remove path with Sys_error _ -> ()); temporary := None; result
  with (Sys_error _ | Unix.Unix_error _) as exception_ ->
    (match !temporary with Some path -> (try Sys.remove path with Sys_error _ -> ()) | None -> ());
    Error (Refused (Printexc.to_string exception_))
let decide t database frame = with_frame t "decide" database frame
let submit t database frame = with_frame t "submit" database frame
