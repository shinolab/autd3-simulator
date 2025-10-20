#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct UpdateFlag(u32);

impl UpdateFlag {
    pub const UPDATE_SLICE_COLOR_MAP: Self = Self(1 << 0);
    pub const UPDATE_SLICE_POS: Self = Self(1 << 1);
    pub const UPDATE_SLICE_SIZE: Self = Self(1 << 2);

    pub const UPDATE_CAMERA: Self = Self(1 << 3);

    pub const UPDATE_TRANS_STATE: Self = Self(1 << 4);
    pub const UPDATE_TRANS_ALPHA: Self = Self(1 << 5);
    pub const UPDATE_TRANS_POS: Self = Self(1 << 6);

    pub const UPDATE_CONFIG: Self = Self(1 << 7);

    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn all() -> Self {
        Self(
            Self::UPDATE_SLICE_COLOR_MAP.0
                | Self::UPDATE_SLICE_POS.0
                | Self::UPDATE_SLICE_SIZE.0
                | Self::UPDATE_CAMERA.0
                | Self::UPDATE_TRANS_STATE.0
                | Self::UPDATE_TRANS_ALPHA.0
                | Self::UPDATE_TRANS_POS.0
                | Self::UPDATE_CONFIG.0,
        )
    }

    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    pub const fn contains(&self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn set(&mut self, other: Self, value: bool) {
        if value {
            self.0 |= other.0;
        } else {
            self.0 &= !other.0;
        }
    }

    pub fn remove(&mut self, other: Self) {
        self.0 &= !other.0;
    }
}
