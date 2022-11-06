//! Emulated USB Host Controller

use std::sync::Arc;

use crate::common::{RWOp, ReadOp, WriteOp};
use crate::hw::ids::pci::{PROPOLIS_XHCI_DEV_ID, VENDOR_OXIDE};
use crate::hw::pci;
use crate::inventory::Entity;

use super::bits;
use super::registers::*;

/// An emulated USB Host Controller attached over PCI
pub struct PciXhci {
    /// PCI device state
    pci_state: pci::DeviceState,
}

impl PciXhci {
    /// Create a new pci-xhci device
    pub fn create() -> Arc<Self> {
        let pci_builder = pci::Builder::new(pci::Ident {
            vendor_id: VENDOR_OXIDE,
            device_id: PROPOLIS_XHCI_DEV_ID,
            sub_vendor_id: VENDOR_OXIDE,
            sub_device_id: PROPOLIS_XHCI_DEV_ID,
            class: pci::bits::CLASS_SERIAL_BUS,
            subclass: pci::bits::SUBCLASS_USB,
            prog_if: pci::bits::PROGIF_USB3,
            ..Default::default()
        });

        let pci_state = pci_builder
            // .add_bar_mmio64(pci::BarN::BAR0, 0x2000)
            .add_custom_cfg(bits::USB_PCI_CFG_OFFSET, bits::USB_PCI_CFG_REG_SZ)
            .finish();

        Arc::new(Self { pci_state })
    }

    /// Handle read of register in USB-specific PCI configuration space
    fn usb_cfg_read(&self, id: UsbPciCfgReg, ro: &mut ReadOp) {
        match id {
            UsbPciCfgReg::SerialBusReleaseNumber => {
                // USB 3.0
                ro.write_u8(0x30);
            }
            UsbPciCfgReg::FrameLengthAdjustment => {
                // We don't support adjusting the SOF cycle
                let fladj = bits::FrameLengthAdjustment(0).with_nfc(true);
                ro.write_u8(fladj.0);
            }
            UsbPciCfgReg::DefaultBestEffortServiceLatencies => {
                // We don't support link power management so return 0
                ro.write_u8(bits::DefaultBestEffortServiceLatencies(0).0);
            }
        }
    }

    /// Handle write to register in USB-specific PCI configuration space
    fn usb_cfg_write(&self, id: UsbPciCfgReg, _wo: &mut WriteOp) {
        match id {
            // Ignore writes to read-only register
            UsbPciCfgReg::SerialBusReleaseNumber => {}

            // We don't support adjusting the SOF cycle
            UsbPciCfgReg::FrameLengthAdjustment => {}

            // We don't support link power management
            UsbPciCfgReg::DefaultBestEffortServiceLatencies => {}
        }
    }

    /// Handle read of memory-mapped host controller register
    fn reg_read(&self, id: Registers, ro: &mut ReadOp) {
        use CapabilityRegisters::*;
        use Registers::*;

        match id {
            Reserved => ro.fill(0),

            // Capability registers
            Cap(CapabilityLength) => {
                // TODO: write offset to operational registers
                // should be cap registers + any space for extended capabilities
                ro.write_u8(0);
            }
            Cap(HciVersion) => {
                // xHCI Version 1.2.0
                ro.write_u16(0x0120);
            }
            Cap(HcStructuralParameters1) => {
                // TODO: set values
                let hcs_params1 = bits::HcStructuralParameters1(0);
                ro.write_u32(hcs_params1.0);
            }
            Cap(HcStructuralParameters2) => {
                // TODO: set values
                let hcs_params2 = bits::HcStructuralParameters2(0);
                ro.write_u32(hcs_params2.0);
            }
            Cap(HcStructuralParameters3) => {
                // TODO: set values
                let hcs_params3 = bits::HcStructuralParameters3(0);
                ro.write_u32(hcs_params3.0);
            }
            Cap(HcCapabilityParameters1) => {
                // TODO: set values
                let hcc_params1 = bits::HcCapabilityParameters1(0);
                ro.write_u32(hcc_params1.0);
            }
            Cap(HcCapabilityParameters2) => {
                // TODO: set values
                let hcc_params2 = bits::HcCapabilityParameters2(0);
                ro.write_u32(hcc_params2.0);
            }
            Cap(DoorbellOffset) => {
                // TODO: write valid doorbell offset
                ro.write_u32(0);
            }
            Cap(RuntimeRegisterSpaceOffset) => {
                // TODO: write valid runtime register space offset
                ro.write_u32(0);
            }
        }
    }

    /// Handle write to memory-mapped host controller register
    fn reg_write(&self, id: Registers, _wo: &mut WriteOp) {
        use Registers::*;

        match id {
            // Ignore writes to reserved bits
            Reserved => {}

            // Capability registers are all read-only; ignore any writes
            Cap(_) => {}
        }
    }
}

impl Entity for PciXhci {
    fn type_name(&self) -> &'static str {
        "pci-xhci"
    }

    fn reset(&self) {
        self.pci_state.reset(self);
    }
}

impl pci::Device for PciXhci {
    fn device_state(&self) -> &pci::DeviceState {
        &self.pci_state
    }

    fn cfg_rw(&self, region: u8, mut rwo: RWOp) {
        assert_eq!(region, bits::USB_PCI_CFG_OFFSET);

        USB_PCI_CFG_REGS.process(
            &mut rwo,
            |id: &UsbPciCfgReg, rwo: RWOp<'_, '_>| match rwo {
                RWOp::Read(ro) => self.usb_cfg_read(*id, ro),
                RWOp::Write(wo) => self.usb_cfg_write(*id, wo),
            },
        )
    }

    fn bar_rw(&self, bar: pci::BarN, mut rwo: RWOp) {
        assert_eq!(bar, pci::BarN::BAR0);

        XHC_REGS.process(
            &mut rwo,
            |id: &Registers, rwo: RWOp<'_, '_>| match rwo {
                RWOp::Read(ro) => self.reg_read(*id, ro),
                RWOp::Write(wo) => self.reg_write(*id, wo),
            },
        )
    }
}
