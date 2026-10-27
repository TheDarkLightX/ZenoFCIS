//! Additional contracts for standard-library operations absent from this Verus pin.
//! Each contract is checked on native samples in value_decoder_std_tests.rs.
extern crate alloc;
use alloc::alloc::Allocator;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use vstd::prelude::*;
use vstd::string::*;
use zeno_fcis_value::decoder_model::*;

verus! {

// Vec::into_boxed_slice preserves the elements in order.
pub assume_specification<T, A: Allocator>[Vec::<T, A>::into_boxed_slice](items: Vec<T, A>)
    -> (boxed: Box<[T], A>) ensures boxed@ == items@;

// String::into_boxed_str preserves the string's characters.
pub assume_specification[String::into_boxed_str](text: String)
    -> (boxed: Box<str>) ensures boxed@ == text@;

// Slice::to_vec clones each element, preserving length and order.
pub assume_specification<T: Clone>[<[T]>::to_vec](items: &[T]) -> (copied: Vec<T>)
    ensures copied@.len() == items@.len(),
        forall|i: int| 0 <= i < items@.len() ==>
            cloned(items@[i], copied@[i]);

// A byte slice is ASCII exactly when each byte is below 128.
pub assume_specification[<[u8]>::is_ascii](bytes: &[u8]) -> (result: bool)
    ensures result == ascii(bytes@);

#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExUtf8Error(core::str::Utf8Error);

// UTF-8 conversion succeeds exactly for valid UTF-8 and preserves the bytes.
pub assume_specification<'a>[core::str::from_utf8](bytes: &'a [u8])
    -> (result: Result<&'a str, core::str::Utf8Error>)
    ensures result.is_ok() == vstd::utf8::valid_utf8(bytes@),
        result matches Ok(text) ==> text.spec_bytes() == bytes@;

}
