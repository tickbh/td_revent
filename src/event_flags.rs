bitflags! {
        flags EventFlags: u64 {
            const FLAG_TIMEOUT 	        = 0b000000000001,
            const FLAG_READ             = 0b000000000010,
            const FLAG_WRITE            = 0b000000000100,
            const FLAG_PERSIST          = 0b000000001000,
            const FLAG_ERROR            = 0b000000010000,
            const FLAG_ACCEPT           = 0b000000100000,
            const FLAG_ENDED            = 0b000001000000,
            const FLAG_READ_PERSIST     = 0b000010000000,
            const FLAG_WRITE_PERSIST    = 0b000100000000,
        }
    }
