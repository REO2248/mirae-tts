//! Connect.pkg - phoneme bigram connectivity matrix (401x401 bytes).
//! Format: [u32 n][u8 x n*n] row-major 0/1 matrix (original loader FUN_0044e810).
use std::io::{self, Read};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ConnectMatrix {
    pub n: usize,
    pub matrix: Vec<u8>,
}

impl ConnectMatrix {
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 4 {
            return None;
        }
        let n = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let expected = 4 + n * n;
        if data.len() < expected {
            return None;
        }
        Some(ConnectMatrix {
            n,
            matrix: data[4..expected].to_vec(),
        })
    }

    pub fn load<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let mut f = std::fs::File::open(path)?;
        let mut data = Vec::new();
        f.read_to_end(&mut data)?;
        Self::parse(&data).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Connect.pkg: malformed header or truncated matrix",
            )
        })
    }

    #[inline]
    pub fn get(&self, left: usize, right: usize) -> u8 {
        if left < self.n && right < self.n {
            self.matrix[left * self.n + right]
        } else {
            0
        }
    }

    #[inline]
    pub fn row(&self, i: usize) -> Option<&[u8]> {
        if i < self.n {
            Some(&self.matrix[i * self.n..(i + 1) * self.n])
        } else {
            None
        }
    }

    #[inline]
    pub fn dim(&self) -> usize {
        self.n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_get() {
        let buf = [2u8, 0, 0, 0, 9, 8, 7, 6];
        let m = ConnectMatrix::parse(&buf).unwrap();
        assert_eq!(m.n, 2);
        assert_eq!(m.get(0, 0), 9);
        assert_eq!(m.get(1, 1), 6);
        assert_eq!(m.get(0, 1), 8);
        assert_eq!(m.get(5, 0), 0);
        assert_eq!(m.row(1), Some(&[7u8, 6][..]));
        assert_eq!(m.row(2), None);
    }

    #[test]
    fn rejects_too_small() {
        assert!(ConnectMatrix::parse(&[0u8; 3]).is_none());
    }
}

