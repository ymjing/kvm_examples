use kvm_bindings::kvm_userspace_memory_region;
use kvm_ioctls::{Kvm, VcpuExit};
use libc::{MAP_ANONYMOUS, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE};

const CODE: &[u8] = &[0x00, 0xd8 /* add al, bl */, 0xf4 /* hlt */];
const CODE_MEMORY_ADDR: u64 = 0x1000;
const CODE_MEMORY_SIZE: usize = 0x1000;

fn main() -> anyhow::Result<()> {
    // Open Kvm
    let kvm = Kvm::new()?;

    // Create VM
    let vm = kvm.create_vm()?;

    // Create vCPU
    let mut vcpu = vm.create_vcpu(0)?;

    // Allocate guest memory on the host
    let host_virtual_address = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            CODE_MEMORY_SIZE,
            PROT_READ | PROT_WRITE | PROT_EXEC,
            MAP_PRIVATE | MAP_ANONYMOUS,
            -1,
            0,
        )
    };

    // Create mapping between host and guest memory
    let slot = 0;
    let mem_region = kvm_userspace_memory_region {
        slot,
        guest_phys_addr: CODE_MEMORY_ADDR,
        memory_size: CODE_MEMORY_SIZE as u64,
        userspace_addr: host_virtual_address as u64,
        flags: 0,
    };
    unsafe { vm.set_user_memory_region(mem_region).unwrap() };

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
    vcpu_regs.rip = CODE_MEMORY_ADDR;
    vcpu_regs.rax = 1;
    vcpu_regs.rbx = 2;
    vcpu_regs.rflags = 2;
    vcpu.set_regs(&vcpu_regs)?;

    loop {
        match vcpu.run()? {
            VcpuExit::Hlt => {
                eprintln!("Received Halt");
                break;
            }
            r => anyhow::bail!("Unexpected exit reason: {:?}", r),
        }
    }

    Ok(())
}
