use std::path::PathBuf;

use rkyv::{Archive, Deserialize, Serialize, rancor::Error, util::AlignedVec, with::AsString};

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct Module {
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub enum Parable {
    Source(String),
    Path(#[rkyv(with = AsString)] PathBuf),
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct Par {
    pub module: Module,
    pub parable: Parable,
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct Pars {
    pub pars: Vec<Par>,
    pub entry: usize,
}

impl Pars {
    pub fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        Ok(rkyv::to_bytes::<Error>(self)?.to_vec())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Pars, Error> {
        let mut aligned = AlignedVec::<16>::new();
        aligned.extend_from_slice(bytes);
        rkyv::from_bytes::<Pars, Error>(&aligned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_bytes() {
        let pars = Pars {
            pars: vec![
                Par {
                    module: Module {
                        path: vec!["main".to_string()],
                    },
                    parable: Parable::Source("main = answer\n".to_string()),
                },
                Par {
                    module: Module {
                        path: vec!["util".to_string(), "io".to_string()],
                    },
                    parable: Parable::Path(PathBuf::from("/app/util/io.par")),
                },
            ],
            entry: 0,
        };

        let bytes = pars.to_bytes().unwrap();
        let restored = Pars::from_bytes(&bytes).unwrap();
        assert_eq!(pars, restored);
    }
}
