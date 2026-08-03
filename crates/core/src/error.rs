//! The port's error type.
//!
//! Deliberately hand-rolled rather than pulled from `thiserror`: this crate has
//! three dependencies and all of them earn their place, so a proc-macro crate
//! for one enum is not a trade worth making.
//!
//! `String` errors were the original shape here and were wrong for the usual
//! reason — a caller cannot match on prose. Every variant below is something a
//! caller might reasonably branch on.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// `polyad` rejects counts upstream also rejects: NaN or <= 0.
    /// Upstream throws here, so the port returns an error rather than
    /// inventing a value — see DECISIONS.md D-015.
    InvalidPolyadCount,

    /// The RPC request was not valid JSON.
    MalformedRequest(String),

    /// A recognised op with a missing or wrong-typed field.
    BadRequest(&'static str),

    /// An op name the protocol does not define.
    UnknownOp(String),

    /// A handle that is not in the slab — freed, reset, or never issued.
    UnknownHandle(u64),

    /// A method name the RPC layer does not expose.
    UnknownMethod { kind: MethodKind, name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    Instance,
    Static,
}

impl fmt::Display for MethodKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MethodKind::Instance => "instance method",
            MethodKind::Static => "static",
        })
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidPolyadCount => {
                f.write_str("Argument to polyad must be a positive number")
            }
            Error::MalformedRequest(e) => write!(f, "bad request json: {e}"),
            Error::BadRequest(what) => write!(f, "bad request: {what}"),
            Error::UnknownOp(op) => write!(f, "unknown op: {op}"),
            Error::UnknownHandle(id) => write!(f, "unknown color handle: {id}"),
            Error::UnknownMethod { kind, name } => write!(f, "unimplemented {kind}: {name}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn polyad_message_matches_upstream_verbatim() {
        // Upstream throws `new Error("Argument to polyad must be a positive
        // number")`. The text is observable, so it is part of the contract.
        assert_eq!(
            Error::InvalidPolyadCount.to_string(),
            "Argument to polyad must be a positive number"
        );
    }

    #[test]
    fn errors_are_matchable_not_just_printable() {
        let e = Error::UnknownMethod {
            kind: MethodKind::Static,
            name: "nope".into(),
        };
        assert!(matches!(e, Error::UnknownMethod { kind: MethodKind::Static, .. }));
        assert_eq!(e.to_string(), "unimplemented static: nope");
    }
}
