//! The on-disk JSON format the CLI reads for `prove`.
//!
//! Keyed by name, matching `tpt_axiom_zk::NamedWitness`, so a reordered or
//! misspelled field is a clear error rather than a silently different
//! statement. Integers are parsed as `i64` and rejected if they do not fit the
//! IR's scalar model, rather than being truncated into a different number.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

use serde::Deserialize;
use tpt_axiom_ir::Scalar;

/// The JSON shapes accepted for a `prove` input file.
///
/// `Split` lets a caller state which values are public; the circuit remains the
/// authority on visibility, so a name in the wrong half is caught downstream by
/// the circuit's own visibility rules rather than trusted from the file.
/// The JSON shapes accepted for a `prove` input file.
///
/// `Split` lets a caller state which values are public; the circuit remains the
/// authority on visibility, so a name in the wrong half is caught downstream by
/// the circuit's own visibility rules rather than trusted from the file.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum InputFile {
    /// Explicit public/secret split.
    Split {
        /// Values the verifier will see.
        public: BTreeMap<String, serde_json::Value>,
        /// Values that must stay hidden.
        secret: BTreeMap<String, serde_json::Value>,
    },
    /// Flat name/value map.
    Flat(BTreeMap<String, serde_json::Value>),
}

impl InputFile {
    /// Flattens the form into one name/value map.
    #[must_use]
    pub fn into_values(self) -> BTreeMap<String, serde_json::Value> {
        match self {
            Self::Flat(map) => map,
            // The split form flattens here because the *circuit* is the
            // authority on which name is public, not the file.
            Self::Split { public, secret } => public.into_iter().chain(secret).collect(),
        }
    }

    /// Recognises the split form structurally.
    ///
    /// A `#[serde(untagged)]` enum cannot do this correctly: the flat variant
    /// would match `{"public": {...}, "secret": {...}}` first and then fail
    /// later with a confusing message about the *value* of `public`. Deciding
    /// here — an object whose keys are all `public`/`secret` and whose values
    /// are objects — gives the split form its intended reading.
    fn from_value(value: serde_json::Value) -> Result<Self, String> {
        let serde_json::Value::Object(map) = value else {
            return Err("the input file must be a JSON object of named values".to_owned());
        };
        let keys_are_visibility = !map.is_empty()
            && map.keys().all(|k| k == "public" || k == "secret")
            && map.values().all(serde_json::Value::is_object);
        if keys_are_visibility {
            let mut public = BTreeMap::new();
            let mut secret = BTreeMap::new();
            for (key, value) in map {
                let serde_json::Value::Object(inner) = value else {
                    unreachable!("checked to be objects");
                };
                let target = if key == "public" {
                    &mut public
                } else {
                    &mut secret
                };
                *target = inner.into_iter().collect();
            }
            return Ok(Self::Split { public, secret });
        }
        Ok(Self::Flat(map.into_iter().collect()))
    }
}

/// A parsed witness: every named value, checked to fit the IR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessFile {
    /// Named values, keyed by circuit input name.
    values: BTreeMap<String, Scalar>,
}

impl WitnessFile {
    /// Reads and parses an input file.
    ///
    /// # Errors
    ///
    /// [`InputError::Read`] if the file cannot be read, [`InputError::Parse`]
    /// if its bytes are not one of the accepted JSON shapes.
    pub fn read(path: &Path) -> Result<Self, InputError> {
        let bytes = std::fs::read(path).map_err(|e| InputError::Read {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        Self::parse(&bytes).map_err(|message| InputError::Parse {
            path: path.display().to_string(),
            message,
        })
    }

    /// Parses the JSON forms into named scalar values.
    ///
    /// # Errors
    ///
    /// Returns a message naming the offending entry when a value is not an
    /// integer or does not fit the IR's `i64` scalar model.
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let raw = InputFile::from_value(value)?.into_values();
        let mut values = BTreeMap::new();
        for (name, value) in raw {
            values.insert(name.clone(), to_scalar(&name, &value)?);
        }
        Ok(Self { values })
    }

    /// The named values.
    #[must_use]
    pub const fn values(&self) -> &BTreeMap<String, Scalar> {
        &self.values
    }

    /// Resolves this file into positional witness values for `ir`.
    ///
    /// # Errors
    ///
    /// As [`tpt_axiom_zk::NamedWitness::resolve_and_check`]: an incomplete
    /// witness, an unknown name, or a value violating the circuit.
    pub fn into_witness(
        self,
        ir: &tpt_axiom_ir::ConstraintSystem,
    ) -> Result<tpt_axiom_zk::WitnessValues, String> {
        let mut witness = tpt_axiom_zk::NamedWitness::new();
        for (name, value) in &self.values {
            witness.set(name, *value);
        }
        witness.resolve_and_check(ir).map_err(|e| e.to_string())
    }
}

/// Converts one JSON value into an IR scalar.
fn to_scalar(name: &str, value: &serde_json::Value) -> Result<Scalar, String> {
    let number = value.as_i64().ok_or_else(|| {
        format!(
            "input `{name}` must be an integer the IR can represent (the scalar model is i64); \
             got {value}"
        )
    })?;
    Ok(number)
}

/// Why an input file could not be turned into a witness.
#[derive(Debug)]
pub enum InputError {
    /// The file could not be read.
    Read {
        /// The path attempted.
        path: String,
        /// The operating system's message.
        message: String,
    },
    /// The file was read but is not a valid input document.
    Parse {
        /// The path read.
        path: String,
        /// The parse or conversion message.
        message: String,
    },
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, message } => write!(f, "cannot read input file `{path}`: {message}"),
            Self::Parse { path, message } => write!(f, "invalid input file `{path}`: {message}"),
        }
    }
}

impl std::error::Error for InputError {}
#[cfg(test)]
mod tests {
    use super::*;
    use tpt_axiom_ir::{ConstraintSystemBuilder, IntType};

    fn sample_ir() -> tpt_axiom_ir::ConstraintSystem {
        let mut b = ConstraintSystemBuilder::new("transfer");
        let sender = b.public_input_typed("sender", IntType::U64);
        let receiver = b.public_input_typed("receiver", IntType::U64);
        let amount = b.secret_input_typed("amount", IntType::U64);
        let surplus = b.sub(sender, amount);
        b.constrain_non_negative(surplus);
        let new_receiver = b.add(receiver, amount);
        let expected = b.add(receiver, amount);
        b.constrain_eq(new_receiver, expected);
        b.build()
    }

    #[test]
    fn both_accepted_forms_resolve_onto_the_same_witness() {
        let ir = sample_ir();
        let flat = WitnessFile::parse(br#"{"sender":50,"receiver":20,"amount":30}"#)
            .expect("valid input")
            .into_witness(&ir)
            .expect("complete witness");
        let split =
            WitnessFile::parse(br#"{"public":{"sender":50,"receiver":20},"secret":{"amount":30}}"#)
                .expect("valid input")
                .into_witness(&ir)
                .expect("complete witness");
        assert_eq!(flat, split);
        assert_eq!(flat.public(), &[50, 20]);
        assert_eq!(flat.secret(), &[30]);
    }

    #[test]
    fn negative_values_are_accepted_for_signed_inputs() {
        let file = WitnessFile::parse(br#"{"x": -5}"#).expect("valid");
        assert_eq!(file.values()["x"], -5);
    }

    #[test]
    fn a_non_integer_value_names_the_input() {
        let error = WitnessFile::parse(br#"{"sender": "fifty"}"#).expect_err("not an integer");
        assert!(error.contains("sender"), "{error}");
    }

    #[test]
    fn a_value_above_the_scalar_model_is_refused_not_wrapped() {
        // 2^63 does not fit the IR's i64 model; it must be named, not
        // truncated into i64::MIN.
        let json = format!(r#"{{"sender": {}}}"#, i128::from(i64::MAX) + 1);
        let error = WitnessFile::parse(json.as_bytes()).expect_err("out of range");
        assert!(error.contains("sender"), "{error}");
        assert!(error.contains("i64"), "the message should say why: {error}");
    }

    #[test]
    fn a_fractional_value_is_refused() {
        let error = WitnessFile::parse(br#"{"sender": 1.5}"#).expect_err("not an integer");
        assert!(error.contains("sender"), "{error}");
    }

    #[test]
    fn a_missing_input_is_reported_when_resolving() {
        let ir = sample_ir();
        let error = WitnessFile::parse(br#"{"sender": 50}"#)
            .expect("valid JSON")
            .into_witness(&ir)
            .expect_err("incomplete witness");
        assert!(error.contains("receiver"), "{error}");
    }

    #[test]
    fn an_unknown_input_is_reported_when_resolving() {
        let ir = sample_ir();
        let error = WitnessFile::parse(br#"{"sender":50,"receiver":20,"amount":30,"amunt":1}"#)
            .expect("valid JSON")
            .into_witness(&ir)
            .expect_err("unknown name");
        assert!(error.contains("amunt"), "{error}");
    }

    #[test]
    fn a_constraint_violating_witness_is_refused() {
        let ir = sample_ir();
        let error = WitnessFile::parse(br#"{"sender":10,"receiver":20,"amount":30}"#)
            .expect("valid JSON")
            .into_witness(&ir)
            .expect_err("amount exceeds sender");
        assert!(error.contains("violates the circuit"), "{error}");
    }

    #[test]
    fn a_missing_file_is_reported_rather_than_panicking() {
        let error =
            WitnessFile::read(Path::new("definitely/not/here.json")).expect_err("no such file");
        assert!(error.to_string().contains("cannot read"), "{error}");
    }

    #[test]
    fn malformed_json_is_reported_with_the_path() {
        let dir = std::env::temp_dir().join("tpt_axiom_cli_inputs_test");
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("bad.json");
        std::fs::write(&path, b"{not json").expect("write");
        let error = WitnessFile::read(&path).expect_err("malformed");
        assert!(error.to_string().contains("bad.json"), "{error}");
        std::fs::remove_file(&path).ok();
    }
}
