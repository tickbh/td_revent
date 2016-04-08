

bitflags! {
        flags EventFlags: u32 {
            const FLAG_TIMEOUT 	 = 0b00000001,
            const FLAG_READ      = 0b00000010,
            const FLAG_WRITE     = 0b00000100,
            const FLAG_PERSIST   = 0b00001000,
        }
    }
