//! Static reading tables: exception words (EXCEPTION_TABLE), unit readings,
//! digit words/prefixes, English digraphs and jamo names — byte dumps from
//! the original Future.exe plus their lookup helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardReading {
    pub main: &'static [u8],
    pub sub: &'static [u8],
    pub sub2: Option<&'static [u8]>,
    pub marker: u8,
    pub morphemes: u8,
    pub f1389: u8,
    pub f1400: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExceptionOutcome {
    Lookup(&'static [u8]),
    Hard(HardReading),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionRule {
    pub input: &'static [u8],
    pub out: ExceptionOutcome,
}

pub static EXCEPTION_TABLE: [ExceptionRule; 60] = [
    ExceptionRule {
        input: &[0xb1, 0xfd, 0xb0, 0xa1],
        out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xb0, 0xa1, 0xca, 0xad]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xb0, 0xd6],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xcb, 0xcb, 0xb0, 0xd6]),
    },
    ExceptionRule {
        input: &[0xc3, 0xcd, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc3, 0xcd, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[
            0xcb, 0xce, 0xb8, 0xc9, 0xc3, 0xf9, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7,
        ]),
    },
    ExceptionRule {
        input: &[0xbd, 0xdb, 0xbc, 0xbf, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xbd, 0xdb, 0xbc, 0xbf, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xbc, 0xad, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xbc, 0xad, 0xc3, 0xcd, 0xbc, 0xec],
        out: ExceptionOutcome::Lookup(&[0xbc, 0xad, 0xc2, 0xd7, 0xca, 0xde, 0xbc, 0xec]),
    },
    ExceptionRule {
        input: &[0xb6, 0xed, 0xb1, 0xfd],
        out: ExceptionOutcome::Lookup(&[0xb6, 0xed, 0xb1, 0xfd, 0xca, 0xad]),
    },
    ExceptionRule {
        input: &[0xb9, 0xbe, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde],
        out: ExceptionOutcome::Lookup(&[0xb9, 0xbe, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8],
        out: ExceptionOutcome::Lookup(&[
            0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7, 0xb4, 0xc7, 0xbc, 0xe8,
        ]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xb8, 0xf5, 0xbc, 0xac, 0xcb, 0xcb],
        out: ExceptionOutcome::Lookup(&[0xb8, 0xf3, 0xb2, 0xf7, 0xbc, 0xac, 0xcb, 0xcb]),
    },
    ExceptionRule {
        input: &[0xb1, 0xfd, 0xcc, 0xae],
        out: ExceptionOutcome::Lookup(&[0xb1, 0xfd, 0xca, 0xef, 0xca, 0xad]),
    },
    ExceptionRule {
        input: &[0xb4, 0xae, 0xb5, 0xd8, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xae, 0xb6, 0xae, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xc0, 0xb2],
        out: ExceptionOutcome::Lookup(&[0xc0, 0xb0, 0xb2, 0xf7]),
    },
    ExceptionRule {
        input: &[0xca, 0xf1],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xb2, 0xf7]),
    },
    ExceptionRule {
        input: &[0xc3, 0xcd, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xc3, 0xcd, 0xb4, 0xaa],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xb4, 0xaa]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xcc, 0xae, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xcc, 0xae, 0xb4, 0xaa],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]),
    },
    ExceptionRule {
        input: &[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xca, 0xbf]),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7],
        out: ExceptionOutcome::Lookup(&[0xb4, 0xdd, 0xc2, 0xd7, 0xca, 0xde, 0xba, 0xb7]),
    },
    ExceptionRule {
        input: &[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa],
        out: ExceptionOutcome::Lookup(&[0xca, 0xef, 0xca, 0xad, 0xb4, 0xaa]),
    },
    ExceptionRule {
        input: &[0xbb, 0xf4, 0xb4, 0xaa],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbb, 0xf4],
            sub: &[0xb4, 0xaa],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xbc, 0xea],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1, 0xbc, 0xe8],
            sub: &[0xb2, 0xf7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xbc, 0xe8, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1, 0xbc, 0xe8],
            sub: &[0xb2, 0xf7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb3, 0xad, 0xb6, 0xb0],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb3, 0xad, 0xb6, 0xae],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xb7, 0xb2],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xb7, 0xb2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xbc, 0xc2, 0xb5, 0xb9],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbc, 0xc2],
            sub: &[0xb5, 0xb9],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xb7, 0xb2, 0xba, 0xb7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xb7, 0xb2, 0xba, 0xb7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xb4, 0xaa],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xb4, 0xaa],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xcb, 0xcb, 0xb5, 0xcf],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xcb, 0xcb, 0xb5, 0xd6],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 5,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xc2, 0xd9, 0xb4, 0xe7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xc2, 0xd7],
            sub: &[0xa4, 0xa2, 0xb4, 0xe7],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xc2, 0xd7, 0xca, 0xde],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xc2, 0xd7],
            sub: &[0xca, 0xde],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb8, 0xf6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb8, 0xf3],
            sub: &[0xa4, 0xa4],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa5],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xa4, 0xa4],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb2, 0xa4],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb1, 0xfd],
            sub: &[0xa4, 0xa4],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xbd, 0xd5],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbd, 0xd3],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb8, 0xf3, 0xbb, 0xa6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb8, 0xf3, 0xbb, 0xa4],
            sub: &[0xa4, 0xa2],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xa1, 0xbc, 0xe8],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xa1],
            sub: &[0xbc, 0xe8],
            sub2: None,
            marker: 4,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb3, 0xad, 0xb0, 0xa1],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb3, 0xad],
            sub: &[0xb0, 0xa1],
            sub2: None,
            marker: 2,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xbc, 0xe8, 0xb6, 0xa6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb4, 0xdd, 0xbc, 0xe8],
            sub: &[0xb6, 0xa6],
            sub2: None,
            marker: 1,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xba, 0xa8, 0xb1, 0xe1],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xba, 0xa8],
            sub: &[0xb1, 0xe1],
            sub2: None,
            marker: 1,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xfb, 0xb6, 0xa6],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xfb],
            sub: &[0xb6, 0xa6],
            sub2: None,
            marker: 2,
            morphemes: 2,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb4, 0xdd, 0xc3, 0xcd, 0xba, 0xb7, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb4, 0xdd, 0xc2, 0xd7],
            sub: &[0xca, 0xde, 0xba, 0xb7],
            sub2: Some(&[0xb2, 0xf7]),
            marker: 4,
            morphemes: 3,
            f1389: 0x15,
            f1400: 0x91,
        }),
    },
    ExceptionRule {
        input: &[0xca, 0xef, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xca, 0xef, 0xb2, 0xf7],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xc0, 0xb0, 0xb2, 0xf7],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xc0, 0xb0, 0xb2, 0xf7],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xcb, 0xfb, 0xb1, 0xe2, 0xb8, 0xf5, 0xb6, 0xf3],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb9, 0xdd, 0xb5, 0xfb, 0xbd, 0xc3],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xca, 0xbf],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb1, 0xd7, 0xb7, 0xe1, 0xb8, 0xf2],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xbc, 0xd6, 0xb7, 0xce],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xbc, 0xd6, 0xb7, 0xce],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb6, 0xf0],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb6, 0xf0],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xbd, 0xbf, 0xec],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xbd, 0xbf, 0xec],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb0, 0xdb, 0xb0, 0xfa],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb0, 0xdb, 0xb0, 0xfa],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
    ExceptionRule {
        input: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0],
        out: ExceptionOutcome::Hard(HardReading {
            main: &[0xb4, 0xf5, 0xb6, 0xe7, 0xbe, 0xe0],
            sub: &[],
            sub2: None,
            marker: 0,
            morphemes: 1,
            f1389: 0,
            f1400: 0,
        }),
    },
];

pub fn lookup_exception(input: &[u8]) -> Option<ExceptionRule> {
    if input.is_empty() {
        return None;
    }
    EXCEPTION_TABLE.iter().find(|r| r.input == input).cloned()
}

pub static UNIT_TABLE: [(&[u8], &[u8]); 24] = [
    (b"m", &[0xb8, 0xa1, 0xc0, 0xbe]),
    (b"cm", &[0xbb, 0xbf, 0xbe, 0xb7, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"mm", &[0xb7, 0xe7, 0xb6, 0xae, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"dm", &[0xb4, 0xe7, 0xbb, 0xa4, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"km", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"fm", &[0xc2, 0xc0, 0xc0, 0xcb, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"nm", &[0xb1, 0xfd, 0xb2, 0xd1, 0xb8, 0xa1, 0xc0, 0xbe]),
    (b"g", &[0xb0, 0xfb, 0xb5, 0xbd]),
    (b"mg", &[0xb7, 0xe7, 0xb6, 0xae, 0xb0, 0xfb, 0xb5, 0xbd]),
    (b"kg", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb0, 0xfb, 0xb5, 0xbd]),
    (b"t", &[0xc0, 0xcd]),
    (b"V", &[0xb8, 0xf6, 0xc0, 0xe2]),
    (b"pV", &[0xc2, 0xaa, 0xbf, 0xb8, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"nV", &[0xb1, 0xfd, 0xb2, 0xd1, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"mV", &[0xb7, 0xe7, 0xb6, 0xae, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"kV", &[0xbf, 0xd4, 0xb5, 0xe1, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"MV", &[0xb8, 0xa1, 0xb0, 0xa1, 0xb8, 0xf6, 0xc0, 0xe2]),
    (b"A", &[0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad]),
    (
        b"pA",
        &[0xc2, 0xaa, 0xbf, 0xb8, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (
        b"nA",
        &[0xb1, 0xfd, 0xb2, 0xd1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (
        b"mA",
        &[0xb7, 0xe7, 0xb6, 0xae, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (
        b"kA",
        &[0xbf, 0xd4, 0xb5, 0xe1, 0xca, 0xb7, 0xc2, 0xbc, 0xca, 0xad],
    ),
    (b"W", &[0xcc, 0xae, 0xc0, 0xe2]),
    (b"pW", &[0xc2, 0xaa, 0xbf, 0xb8, 0xcc, 0xae, 0xc0, 0xe2]),
];

/// Synthetic unit extensions for number_unit tests (not present in Future.exe original table).
/// Kept separate so byte-exact claims apply to UNIT_TABLE_CORE only.
pub static UNIT_TABLE_SYNTHETIC: &[(&[u8], &[u8])] = &[
    (b"Hz", &[0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf]), // 헤르츠 (KeyPad.Ebd)
    (
        b"kHz",
        &[0xbf, 0xd4, 0xb5, 0xe1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf],
    ),
    (
        b"MHz",
        &[0xb8, 0xa1, 0xb0, 0xa1, 0xc3, 0xd7, 0xb6, 0xa3, 0xbe, 0xaf],
    ),
    (b"ppm", &[0xc2, 0xaa, 0xc2, 0xaa, 0xcb, 0xea]), // 피피엠 (KeyPad.Ebd)
    (b"dB", &[0xb4, 0xe7, 0xbb, 0xa4, 0xb9, 0xd9]),  // 데시벨 (KeyPad.Ebd)
    (b"J", &[0xbc, 0xd4]),                           // 줄 (KeyPad.Ebd)
    (b"F", &[0xc2, 0xb2, 0xb5, 0xcd, 0xb4, 0xc5]),   // 패러드 (KeyPad.Ebd)
    (b"N", &[0xb2, 0xee, 0xc0, 0xc0]),               // 뉴턴 (KeyPad.Ebd)
    (b"Pa", &[0xc1, 0xc4, 0xba, 0xf7, 0xbe, 0xf5]),  // 파스칼 (KeyPad.Ebd)
];

pub fn unit_reading(unit: &[u8]) -> Option<&'static [u8]> {
    if let Some((_, r)) = UNIT_TABLE.iter().find(|(u, _)| *u == unit) {
        return Some(*r);
    }
    UNIT_TABLE_SYNTHETIC
        .iter()
        .find(|(u, _)| *u == unit)
        .map(|(_, r)| *r)
}

pub fn unit_match(unit: &[u8]) -> bool {
    const MATCH: &[&[u8]] = &[
        &[0xa1, 0xd5], // 》
        &[0xa2, 0xb9], // ≫
        b">",
        &[0xa1, 0xd3], // 〉
        b"m",
        b"cm",
        b"mm",
        b"dm",
        b"km",
        b"fm",
        b"nm",
        b"g",
        b"mg",
    ];
    MATCH.contains(&unit)
}

pub static DIGIT_WORDS: [&[u8]; 40] = [
    &[0xc2, 0xd9],
    &[0xbb, 0xab],
    &[0xb9, 0xca],
    &[0xbd, 0xe7],
    &[0xb6, 0xed],
    &[0xca, 0xcd],
    &[0xbc, 0xbf],
    &[0xca, 0xde, 0xb5, 0xcd],
    &[0xba, 0xe3],
    &[0xb7, 0xb8],
    &[0xca, 0xde],
    &[],           // sentinel
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[],           // sentinel
    &[],           // NULL
    &[0xb1, 0xb6],
    &[0xb0, 0xd7],
    &[0xc4, 0xfa],
    &[0xbc, 0xb3],
    &[0xb7, 0xba, 0xb1, 0xa4],
    &[0xb8, 0xd2],
    &[0xb8, 0xde],
    &[0xbb, 0xf6],
    &[0xb8, 0xef],
    &[0xbd, 0xea],
    &[0xba, 0xac],
    &[0xbb, 0xf4, 0xb5, 0xf1],
    &[0xbe, 0xa2],
    &[0xbe, 0xc1],
    &[0xbe, 0xf4],
    &[0xbf, 0xb8],
    &[0xc0, 0xd2],
    &[0xc4, 0xda],
    &[0xba, 0xa6],
    &[0xca, 0xb2],
    &[0xc0, 0xcd],
    &[0xb0, 0xa1, 0xbc, 0xe8],
];

pub static DIGIT_PREFIXES: [&[u8]; 40] = [
    &[0xa4, 0xa2], // ㄴ
    &[0xa4, 0xa4], // ㄹ
    &[0xa4, 0xa6], // ㅂ
    &[0xa4, 0xa5], // ㅁ
    &[],           // sentinel
    &[],           // NULL
    &[0xb1, 0xb6],
    &[0xb0, 0xd7],
    &[0xc4, 0xfa],
    &[0xbc, 0xb3],
    &[0xb7, 0xba, 0xb1, 0xa4],
    &[0xb8, 0xd2],
    &[0xb8, 0xde],
    &[0xbb, 0xf6],
    &[0xb8, 0xef],
    &[0xbd, 0xea],
    &[0xba, 0xac],
    &[0xbb, 0xf4, 0xb5, 0xf1],
    &[0xbe, 0xa2],
    &[0xbe, 0xc1],
    &[0xbe, 0xf4],
    &[0xbf, 0xb8],
    &[0xc0, 0xd2],
    &[0xc4, 0xda],
    &[0xba, 0xa6],
    &[0xca, 0xb2],
    &[0xc0, 0xcd],
    &[0xb0, 0xa1, 0xbc, 0xe8],
    &[0xb1, 0xac],
    &[0xb2, 0xd6],
    &[0xb4, 0xb0],
    &[0xb8, 0xdc, 0xc9, 0xe3],
    &[0xb9, 0xc9],
    &[0xbb, 0xa4, 0xb0, 0xa3],
    &[0xbc, 0xd1],
    &[0xbc, 0xb3, 0xb8, 0xf3],
    &[0xc9, 0xe3],
    &[0xb7, 0xcd],
    &[0xb6, 0xf0],
    &[0xb7, 0xf4],
];

pub fn digit_word_hit(input: &[u8]) -> Option<usize> {
    DIGIT_WORDS
        .iter()
        .position(|w| !w.is_empty() && contains_bytes(input, w))
}

pub fn digit_prefix_len(input: &[u8]) -> usize {
    for w in DIGIT_PREFIXES.iter() {
        if w.is_empty() {
            continue;
        }
        if let Some(pos) = find_bytes(input, w) {
            return pos + 1;
        }
    }
    0
}

fn find_bytes(input: &[u8], pat: &[u8]) -> Option<usize> {
    if pat.is_empty() || pat.len() > input.len() {
        return None;
    }
    input.windows(pat.len()).position(|w| w == pat)
}

fn contains_bytes(input: &[u8], pat: &[u8]) -> bool {
    find_bytes(input, pat).is_some()
}

pub fn special_to_key_char(v: u16) -> Option<u8> {
    let u = v & 0x7fff;
    match u {
        0x30..=0x39 => Some((u as u8) + 0x16),
        0x2d => Some(0x45),
        0x2e => Some(0x44),
        _ => None,
    }
}

pub static DIGRAPHS: [&[u8]; 28] = [
    b"es", b"th", b"qu", b"nk", b"dg", b"oo", b"ee", b"oy", b"ay", b"ew", b"au", b"ei", b"ur",
    b"er", b"tia", b"wor", b"old", b"ind", b"igh", b"our", b"ear", b"ure", b"ire", b"are", b"ast",
    b"asp", b"ant", b"aff",
];

pub static DIGRAPH_READINGS: [&[u8]; 22] = [
    &[0xcb, 0xcb],
    &[0xca, 0xef, 0xcb, 0xcb],
    &[0xcb, 0xe6, 0xcb, 0xcb],
    &[0xca, 0xcc],
    &[0xbb, 0xd5, 0xca, 0xcc],
    &[0xbb, 0xd5, 0xca, 0xef, 0xcb, 0xa7],
    &[0xcc, 0xb8],
    &[0xca, 0xef, 0xcb, 0xaa, 0xa4, 0xa3],
    &[0xca, 0xef, 0xcb, 0xa7, 0xba, 0xf7, 0xa4, 0xac],
    &[0xa4, 0xa3], // ㄷ
    &[],
    &[],
    &[0xcb, 0xb1, 0xca, 0xcc],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[],
    &[0xca, 0xad, 0xa4, 0xae],
    &[],
    &[0xbc, 0xad],
];

pub static JAMO_READINGS: [(&[u8], &[u8]); 11] = [
    (&[0xa4, 0xad], &[0xa4, 0xad]), // ㅍ
    (&[0xa4, 0xa2], &[0xa4, 0xa2]), // ㄴ
    (&[0xa4, 0xa4], &[0xa4, 0xa4]), // ㄹ
    (&[0xa4, 0xa7], &[0xa4, 0xa7]), // ㅅ
    (&[0xa4, 0xb2], &[0xa4, 0xb2]), // ㅆ
    (&[0xa4, 0xa6], &[0xa4, 0xa6]), // ㅂ
    (&[0xa4, 0xa9], &[0xa4, 0xa9]), // ㅈ
    (&[0xa4, 0xa8], &[0xa4, 0xa8]), // ㅇ
    (&[0xa4, 0xab], &[0xa4, 0xab]), // ㅋ
    (&[0xa4, 0xac], &[0xa4, 0xac]), // ㅌ
    (&[0xa4, 0xaa], &[0xa4, 0xaa]), // ㅊ
];
