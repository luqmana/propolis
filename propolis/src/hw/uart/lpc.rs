use std::sync::{Arc, Mutex, Weak};

use super::base::Uart;
use crate::chardev::*;
use crate::common::*;
use crate::dispatch::DispCtx;
use crate::instance;
use crate::intr_pins::{IntrPin, LegacyPin};
use crate::pio::{PioBus, PioDev};

use serde::{Deserialize, Serialize};

pub const REGISTER_LEN: usize = 8;

struct UartState {
    uart: Uart,
    irq_pin: LegacyPin,
    auto_discard: bool,
}

impl UartState {
    fn sync_intr_pin(&self) {
        if self.uart.intr_state() {
            self.irq_pin.assert()
        } else {
            self.irq_pin.deassert()
        }
    }
}

pub struct LpcUart {
    state: Mutex<UartState>,
    notify_readable: NotifierCell<dyn Source>,
    notify_writable: NotifierCell<dyn Sink>,
}

impl LpcUart {
    pub fn new(irq_pin: LegacyPin) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(UartState {
                uart: Uart::new(),
                irq_pin,
                auto_discard: true,
            }),
            notify_readable: NotifierCell::new(),
            notify_writable: NotifierCell::new(),
        })
    }
    pub fn attach(self: &Arc<Self>, bus: &PioBus, port: u16) {
        bus.register(
            port,
            REGISTER_LEN as u16,
            Arc::downgrade(self) as Weak<dyn PioDev>,
            0,
        )
        .unwrap();
    }
    fn reset(&self) {
        let mut state = self.state.lock().unwrap();
        state.uart.reset();
        state.sync_intr_pin();
    }
}

impl Sink for LpcUart {
    fn write(&self, data: u8, _ctx: &DispCtx) -> bool {
        let mut state = self.state.lock().unwrap();
        let res = state.uart.data_write(data);
        state.sync_intr_pin();
        res
    }
    fn set_notifier(&self, f: Option<SinkNotifier>) {
        self.notify_writable.set(f);
    }
}
impl Source for LpcUart {
    fn read(&self, _ctx: &DispCtx) -> Option<u8> {
        let mut state = self.state.lock().unwrap();
        let res = state.uart.data_read();
        state.sync_intr_pin();
        res
    }
    fn discard(&self, count: usize, _ctx: &DispCtx) -> usize {
        let mut state = self.state.lock().unwrap();
        let mut discarded = 0;
        while discarded < count {
            if let Some(_val) = state.uart.data_read() {
                discarded += 1;
            } else {
                break;
            }
        }
        state.sync_intr_pin();
        discarded
    }
    fn set_notifier(&self, f: Option<SourceNotifier>) {
        self.notify_readable.set(f);
    }
    fn set_autodiscard(&self, active: bool) {
        let mut state = self.state.lock().unwrap();
        state.auto_discard = active;
    }
}

impl PioDev for LpcUart {
    fn pio_rw(&self, _port: u16, _ident: usize, rwo: RWOp, ctx: &DispCtx) {
        assert!(rwo.offset() < REGISTER_LEN);
        assert!(rwo.len() != 0);
        let mut state = self.state.lock().unwrap();
        let readable_before = state.uart.is_readable();
        let writable_before = state.uart.is_writable();

        match rwo {
            RWOp::Read(ro) => {
                ro.write_u8(state.uart.reg_read(ro.offset() as u8));
            }
            RWOp::Write(wo) => {
                state.uart.reg_write(wo.offset() as u8, wo.read_u8());
            }
        }
        if state.auto_discard {
            while let Some(_val) = state.uart.data_read() {}
        }

        state.sync_intr_pin();

        let read_notify = !readable_before && state.uart.is_readable();
        let write_notify = !writable_before && state.uart.is_writable();

        // The uart state lock cannot be held while dispatching notifications since those callbacks
        // could immediately attempt to read/write the pending data.
        drop(state);
        if read_notify {
            self.notify_readable.notify(self as &dyn Source, ctx);
        }
        if write_notify {
            self.notify_writable.notify(self as &dyn Sink, ctx);
        }
    }
}
impl Entity for LpcUart {
    fn state_transition(
        &self,
        next: instance::State,
        _target: Option<instance::State>,
        _ctx: &DispCtx,
    ) {
        if next == instance::State::Reset {
            self.reset();
        }
    }

    fn serialize(&self, _record: &crate::inventory::Record) -> Box<dyn erased_serde::Serialize> {
        Box::new(LpcUartState {})
    }
}

#[derive(Deserialize, Serialize)]
struct LpcUartState {}