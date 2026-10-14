(** A Linux OCaml 4.14.1 consumer of the actual generated Rust application.
    Frames and reports remain opaque; this module does not evaluate decisions. *)
type t
type frame
type scalar = Int of int64 | Bool of bool | Variant of string
type report = { exit : int; stdout : string; stderr : string }
type error = Native of report | Refused of string | Unknown of string
val profile : string
val max_frame_bytes : int
val max_output_bytes : int
val create : native:string -> version:string -> (t, error) result
(** [version] is the exact installed application's frozen version JSON. *)
val frame : t -> (string * scalar) list -> (frame, error) result
val frame_of_bytes : bytes -> (frame, error) result
(** Copies opaque bytes, including malformed bytes for native refusal checks. *)
val frame_bytes : frame -> bytes
val frame_of_report : string -> (frame, error) result
(** Decodes only the exact deterministic native frame report and lowercase hex. *)
val init : t -> string -> (report, error) result
val state : t -> string -> (report, error) result
val history : t -> string -> (report, error) result
val decide : t -> string -> frame -> (report, error) result
val submit : t -> string -> frame -> (report, error) result
(** A transport [Unknown] during submit may follow a commit. Do not retry;
    reconcile with the actual native state/history operation. *)
