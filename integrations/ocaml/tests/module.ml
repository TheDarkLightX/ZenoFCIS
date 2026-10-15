open Zeno_scalar
let get = function Ok value -> value | Error _ -> failwith "unexpected refusal"
let () =
  let original = Bytes.of_string "opaque" in
  let frame = get (frame_of_bytes original) in
  Bytes.set original 0 'X';
  assert (frame_bytes frame = Bytes.of_string "opaque");
  let returned = frame_bytes frame in
  Bytes.set returned 0 'Y';
  assert (frame_bytes frame = Bytes.of_string "opaque");
  assert (match frame_of_bytes (Bytes.create (max_frame_bytes + 1)) with Error (Refused _) -> true | _ -> false);
  let empty_report = "{\"schema\":\"zeno-fcis/native-scalar-frame/1\",\"profile\":\"zeno-fcis/native-scalar-consumer/1\",\"frame\":\"00ff\"}\n" in
  assert (frame_bytes (get (frame_of_report empty_report)) = Bytes.of_string "\000\255");
  assert (match frame_of_report (empty_report ^ "\n") with Error (Refused _) -> true | _ -> false);
  print_endline "OCaml opaque ownership and strict report groups: 2 passed"
