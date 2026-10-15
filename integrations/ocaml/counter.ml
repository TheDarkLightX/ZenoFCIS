(* Installed worked application: delegates every operation to Zeno_scalar. *)
open Zeno_scalar
let fail message = prerr_endline message; exit 65
let value = function Ok value -> value | Error error ->
  (match error with
   | Native report -> print_string report.stdout; prerr_string report.stderr; exit report.exit
   | Refused message -> fail ("refused: " ^ message)
   | Unknown message -> prerr_endline ("unknown: " ^ message); exit 66)
let read_file limit path =
  let channel = open_in_bin path in
  Fun.protect ~finally:(fun () -> close_in_noerr channel) (fun () ->
    let buffer = Buffer.create 128 and bytes = Bytes.create 4096 in
    let rec read () =
      let count = input channel bytes 0 (Bytes.length bytes) in
      if count = 0 then Buffer.contents buffer
      else if Buffer.length buffer + count > limit then fail "file exceeds declared byte cap"
      else (Buffer.add_subbytes buffer bytes 0 count; read ()) in
    read ())
let write_file path bytes =
  let output = open_out_bin path in
  Fun.protect ~finally:(fun () -> close_out_noerr output) (fun () ->
    output_bytes output bytes; flush output)
let argument word =
  let separator = match String.index_opt word '=' with
    | Some index -> index | None -> fail "scalar argument needs NAME=KIND:VALUE" in
  let name = String.sub word 0 separator in
  let scalar = String.sub word (separator + 1) (String.length word - separator - 1) in
  let kind, text = match String.index_opt scalar ':' with
    | Some index -> String.sub scalar 0 index,
                    String.sub scalar (index + 1) (String.length scalar - index - 1)
    | None -> fail "scalar argument needs i64:, bool: or enum:" in
  name, (match kind, text with
    | "i64", text -> (try Int (Int64.of_string text) with Failure _ -> fail "invalid exact i64")
    | "bool", "true" -> Bool true | "bool", "false" -> Bool false
    | "enum", text -> Variant text
    | _ -> fail "unsupported scalar form (UInt/bytes/text/payload values are refused)")
let report result =
  let report = value result in
  print_string report.stdout; prerr_string report.stderr; exit report.exit
let () =
  try match Array.to_list Sys.argv with
  | [_; "decode-report"; input; output] ->
      let frame = value (frame_of_report (read_file (2 * max_frame_bytes + 256) input)) in
      write_file output (frame_bytes frame)
  | _ :: native :: version_file :: operation :: arguments ->
      let client = value (create ~native ~version:(read_file max_output_bytes version_file)) in
      (match operation, arguments with
       | "frame", output :: fields ->
           let frame = value (frame client (List.map argument fields)) in
           write_file output (frame_bytes frame)
       | "init", [database] -> report (init client database)
       | "state", [database] -> report (state client database)
       | "history", [database] -> report (history client database)
       | ("decide" | "submit"), [database; framed] ->
           let frame = value (frame_of_bytes (Bytes.of_string (read_file max_frame_bytes framed))) in
           report ((if operation = "submit" then submit else decide) client database frame)
       | _ -> fail "usage: counter NATIVE VERSION.json frame FILE NAME=KIND:VALUE... | init/state/history DB | decide/submit DB FRAME")
  | _ -> fail "usage: counter NATIVE VERSION.json OPERATION ..."
  with Sys_error message -> fail message
