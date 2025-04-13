use core::fmt;
use std::borrow::Cow;

#[derive(Clone, Debug)]
pub struct Port {
    pub direction: Direction,
    pub kind: Kind,
    pub name: Cow<'static, str>,
}

#[derive(Copy, Clone, Debug)]
pub enum Direction {
    Input,
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Audio(Audio),
    Event(Event),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub description: Cow<'static, str>,
    pub size: usize,
    pub align: usize,
}

#[derive(Debug, Clone)]
pub struct Audio {
    pub description: Cow<'static, str>,
    pub num_channels: usize,
}

impl PartialEq for Audio {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name() && self.num_channels == other.num_channels
    }
}

impl fmt::Display for Audio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl Eq for Audio {}

impl Audio {
    /// Create a new (custom) description.
    pub fn new(name: &str, num_channels: usize) -> Self {
        Self {
            description: Cow::Owned(name.to_lowercase()),
            num_channels,
        }
    }

    /// Get the name.
    pub fn name(&self) -> &str {
        self.description.as_ref()
    }

    /// Get the number of channels.
    pub fn num_channels(&self) -> usize {
        self.num_channels
    }
}

/// Mono.
pub const MONO: Audio = Audio {
    description: Cow::Borrowed("mono"),
    num_channels: 1,
};

/// Stereo (L, R)
pub const STEREO: Audio = Audio {
    description: Cow::Borrowed("stereo"),
    num_channels: 2,
};

/// Mid-side (L + R, L - R)
pub const MID_SIDE: Audio = Audio {
    description: Cow::Borrowed("mid-side"),
    num_channels: 2,
};

/// 5.0 surround (L, C, R, Ls, Rs)
pub const SURROUND_5_0: Audio = Audio {
    description: Cow::Borrowed("surround-5.0"),
    num_channels: 5,
};

/// 5.1 surround (L, C, R, Ls, Rs, LFE)
pub const SURROUND_5_1: Audio = Audio {
    description: Cow::Borrowed("surround-5.1"),
    num_channels: 6,
};

pub const SURROUND_5_1_4: Audio = Audio {
    description: Cow::Borrowed("surround-5.1.4"),
    num_channels: 10,
};

pub const SURROUND_7_1: Audio = Audio {
    description: Cow::Borrowed("surround-7.1"),
    num_channels: 8,
};

pub const SURROUND_7_1_4: Audio = Audio {
    description: Cow::Borrowed("surround-7.1.4"),
    num_channels: 12,
};

/// 0th order ambisonics (ACN encoding)
pub const ACN_0: Audio = Audio {
    description: Cow::Borrowed("acn-0"),
    num_channels: 1,
};

/// 1st order ambisonics (ACN encoding)
pub const ACN_1: Audio = Audio {
    description: Cow::Borrowed("acn-1"),
    num_channels: 4,
};

/// 2nd order ambisonics (ACN encoding)
pub const ACN_2: Audio = Audio {
    description: Cow::Borrowed("acn-2"),
    num_channels: 9,
};

/// 3rd order ambisonics (ACN encoding)
pub const ACN_3: Audio = Audio {
    description: Cow::Borrowed("acn-3"),
    num_channels: 16,
};

pub const UMP: Event = Event {
    description: Cow::Borrowed("ump"),
    size: 4,
    align: 4,
};
