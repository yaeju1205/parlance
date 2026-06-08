use std::path::PathBuf;

use rkyv::{Archive, Deserialize, Serialize, rancor::Error, util::AlignedVec, with::AsString};

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub enum FileContent {
    Source(String),
    Path(#[rkyv(with = AsString)] PathBuf),
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct VirtualFile {
    pub path: String,
    pub content: FileContent,
}

#[derive(Debug, Clone, PartialEq, Archive, Serialize, Deserialize)]
pub struct Pars {
    pub files: Vec<VirtualFile>,
    pub entry: String,
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
            files: vec![
                VirtualFile {
                    path: "/greet/main.par".to_string(),
                    content: FileContent::Source("main = answer\n".to_string()),
                },
                VirtualFile {
                    path: "/greet/util/io.par".to_string(),
                    content: FileContent::Path(PathBuf::from("/app/util/io.par")),
                },
            ],
            entry: "/greet/main.par".to_string(),
        };

        let bytes = pars.to_bytes().unwrap();
        let restored = Pars::from_bytes(&bytes).unwrap();
        assert_eq!(pars, restored);
    }
}
