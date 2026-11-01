use crate::BenchResult;
use serde::{Deserialize, Deserializer};

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputConfig {
    value: String,
    #[serde(default, deserialize_with = "present_size")]
    size_bytes: Option<usize>,
}
fn present_size<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<usize>, D::Error> {
    usize::deserialize(deserializer).map(Some)
}
impl Default for InputConfig {
    fn default() -> Self {
        Self {
            value: "benchmark-input!".to_owned(),
            size_bytes: None,
        }
    }
}
pub struct Input {
    pub bytes: Vec<u8>,
    pub mode: &'static str,
}
impl InputConfig {
    pub fn prepare(&self) -> BenchResult<Input> {
        let Some(size_bytes) = self.size_bytes else {
            return Ok(Input {
                bytes: self.value.as_bytes().to_vec(),
                mode: "text",
            });
        };
        if self.value.is_empty() || size_bytes == 0 {
            return Err(
                "size_bytes must be positive and value must be nonempty when size_bytes is set"
                    .into(),
            );
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size_bytes)
            .map_err(|e| format!("input allocation failed: {e}"))?;
        while bytes.len() < size_bytes {
            let n = (size_bytes - bytes.len()).min(self.value.len());
            bytes.extend_from_slice(&self.value.as_bytes()[..n]);
        }
        Ok(Input {
            bytes,
            mode: "repeat",
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_modes_and_validation() {
        assert_eq!(InputConfig::default().prepare().unwrap().bytes.len(), 16);
        let text: InputConfig = serde_yaml::from_str("value: 'şA'\n# size_bytes: 4").unwrap();
        assert_eq!(text.prepare().unwrap().bytes, "şA".as_bytes());
        let repeat: InputConfig = serde_yaml::from_str("value: 'şA'\nsize_bytes: 4").unwrap();
        assert_eq!(repeat.prepare().unwrap().bytes, [0xc5, 0x9f, b'A', 0xc5]);
        let large: InputConfig = serde_yaml::from_str("value: a\nsize_bytes: 1048576").unwrap();
        assert_eq!(large.prepare().unwrap().bytes.len(), 1048576);
        for yaml in ["value: ''\nsize_bytes: 1", "value: a\nsize_bytes: 0"] {
            assert!(serde_yaml::from_str::<InputConfig>(yaml)
                .unwrap()
                .prepare()
                .is_err());
        }
        for yaml in [
            "value: a\nsize_bytes: null",
            "value: a\nsize_bytes: -1",
            "value: a\nsize_bytes: abc",
            "value: a\nmode: text",
        ] {
            assert!(serde_yaml::from_str::<InputConfig>(yaml).is_err());
        }
    }
}
