use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! string_id {
    ($name:ident, $prefix:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            #[must_use]
            pub fn generate() -> Self {
                Self(format!("{}_{}", $prefix, Uuid::new_v4().simple()))
            }

            #[must_use]
            pub fn from_string(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_id!(SessionId, "sess");
string_id!(TaskId, "task");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_ids_are_unique_and_prefixed() {
        let a = SessionId::generate();
        let b = SessionId::generate();
        assert_ne!(a, b);
        assert!(a.as_str().starts_with("sess_"));
        assert!(TaskId::generate().as_str().starts_with("task_"));
    }
}
