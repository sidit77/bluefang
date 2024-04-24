use crate::hci::{Error, Hci, Opcode, OpcodeGroup};
use crate::hci::consts::Lap;

impl Hci {

    /// Start the inquiry process to discover other Bluetooth devices in the vicinity.
    /// ([Vol 4] Part E, Section 7.1.1).
    ///
    /// # Parameters
    /// - `time`: The duration of the inquiry process in 1.28s units. Range: 1-30.
    /// - `max_responses`: The maximum number of responses to receive. 0 means no limit.
    pub async fn inquiry(&self, lap: Lap, time: u8, max_responses: u8) -> Result<(), Error> {
        self.call_with_args(Opcode::new(OpcodeGroup::LinkControl, 0x0001), |p| {
            p.u24(lap);
            p.u8(time);
            p.u8(max_responses);
        }).await?;
        // TODO return channel for inquiry results
        Ok(())
    }
}

