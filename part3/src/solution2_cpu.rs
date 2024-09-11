//use byteorder::{ByteOrder, LittleEndian};
use kvm_bindings::{
    kvm_guest_debug, kvm_userspace_memory_region, KVM_GUESTDBG_ENABLE, KVM_GUESTDBG_USE_HW_BP,
};
use kvm_ioctls::{Kvm, VcpuExit};
use libc::{MAP_ANONYMOUS, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE};

const CODE: &[u8] = &[
    0x0f, 0xc7, 0xf0, /* rdrand ax */
    0xbb, 0x10, 0x10, /* mov bx, 0x1010 */
    0x89, 0x07, /* mov WORD PTR [bx], ax */
    0xf4, /* hlt */
];

const CODE_MEMORY_HVA: u64 = 0xcafe_0000;
const CODE_MEMORY_GPA: u64 = 0x1000;
const CODE_MEMORY_SIZE: usize = 0x1000;

fn main() -> anyhow::Result<()> {
    // Open Kvm
    let kvm = Kvm::new()?;

    // Create VM
    let vm = kvm.create_vm()?;

    // Create vCPU
    let mut vcpu = vm.create_vcpu(0)?;

    let mut dbg = kvm_guest_debug {
        control: KVM_GUESTDBG_ENABLE | KVM_GUESTDBG_USE_HW_BP,
        ..Default::default()
    };
    dbg.arch.debugreg[0] = 0x1003; // after rdrand
    dbg.arch.debugreg[7] = 0x0600;
    // Set global breakpoint enable flag
    dbg.arch.debugreg[7] |= 2;
    vcpu.set_guest_debug(&dbg)?;

    // Allocate and prepare the guest memory
    let host_virtual_address = unsafe {
        libc::mmap(
            CODE_MEMORY_HVA as *mut libc::c_void,
            CODE_MEMORY_SIZE,
            PROT_READ | PROT_WRITE | PROT_EXEC,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    eprintln!(
        "Mapped guest memory at: 0x{:x}",
        host_virtual_address as u64
    );

    // Create mapping between host and guest memory
    let slot = 0;
    let mem_region = kvm_userspace_memory_region {
        slot,
        guest_phys_addr: CODE_MEMORY_GPA,
        memory_size: CODE_MEMORY_SIZE as u64,
        userspace_addr: host_virtual_address as u64,
        flags: 0,
    };
    unsafe { vm.set_user_memory_region(mem_region)? };

    // Host writes to guest memory.
    unsafe {
        libc::memcpy(
            host_virtual_address,
            CODE.as_ptr() as *const libc::c_void,
            CODE.len(),
        );
    }

    // Set initial vCPU registers
    let mut vcpu_sregs = vcpu.get_sregs()?;
    vcpu_sregs.cs.base = 0;
    vcpu_sregs.cs.selector = 0;
    vcpu.set_sregs(&vcpu_sregs)?;

    let mut vcpu_regs = vcpu.get_regs()?;
    vcpu_regs.rip = CODE_MEMORY_GPA;
    vcpu_regs.rflags = 2;
    vcpu.set_regs(&vcpu_regs)?;

    loop {
        match vcpu.run()? {
            VcpuExit::Hlt => {
                eprintln!("Received Halt");
                break;
            }
            VcpuExit::Debug(d) => {
                let ax = vcpu.get_regs()?.rax;
                eprintln!("rip: 0x{:x}, rax: 0x{:x}", d.pc, ax);
                vcpu.set_guest_debug(&kvm_guest_debug::default())?;
            }
            r => anyhow::bail!("Unexpected exit reason: {:?}", r),
        }
    }

    Ok(())
}
