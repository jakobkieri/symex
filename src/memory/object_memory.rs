//! Object memory
//!
use std::collections::BTreeMap;
use tracing::trace;

use crate::{
    memory::{linear_allocator::LinearAllocator, Memory, MemoryError},
    smt::{DContext, DExpr, DSolver, Expression, SolverContext},
};

#[derive(Debug, Clone)]
pub struct MemoryObject {
    //name: String,
    address: u64,

    size: u64,

    bv: DExpr,
}

#[derive(Debug, Clone)]
pub struct ObjectMemory {
    ctx: &'static DContext,

    /// Allocator is used to generate new addresses.
    allocator: LinearAllocator,

    objects: BTreeMap<u64, MemoryObject>,

    solver: DSolver,

    ptr_size: u32,

    alloc_id: usize,
}

impl Memory for ObjectMemory {
    fn allocate(&mut self, bits: u64, align: u64) -> Result<u64, MemoryError> {
        let (addr, _bytes) = self.allocator.get_address(bits, align)?;

        let name = format!("alloc{}", self.alloc_id);
        // println!("Allocate {name} at addr: 0x{addr:x}, size_bits: {bits} bytes");
        self.alloc_id += 1;

        let obj = MemoryObject {
            //name: name.clone(),
            address: addr,
            size: bits,
            bv: self.ctx.unconstrained(bits as u32, &name),
        };
        self.objects.insert(addr, obj);

        Ok(addr)
    }

    fn read(&self, addr: &DExpr, bits: u32) -> Result<DExpr, MemoryError> {
        trace!("read addr={addr:?}, bits={bits}");
        assert_eq!(addr.len(), self.ptr_size, "passed wrong sized address");

        let (addr, value) = self.resolve_address(addr)?.unwrap();
        let offset = (addr - value.address) as u32 * 8;
        let val = value.bv.slice(offset, offset + bits - 1);
        Ok(val)
    }

    fn write(&mut self, addr: &DExpr, value: DExpr) -> Result<(), MemoryError> {
        trace!("write addr={addr:?}, value={value:?}");
        assert_eq!(addr.len(), self.ptr_size, "passed wrong sized address");

        let (addr, val) = self.resolve_address_mut(addr)?.unwrap();
        let offset = (addr - val.address) * 8;

        if value.len() == val.size as u32 {
            val.bv = value;
        } else {
            val.bv = val.bv.replace_part(offset as u32, value);
        }

        Ok(())
    }
}

impl ObjectMemory {
    pub fn new(ctx: &'static DContext, ptr_size: u32, solver: DSolver) -> Self {
        Self {
            ctx,
            allocator: LinearAllocator::new(),
            objects: BTreeMap::new(),
            ptr_size,
            alloc_id: 0,
            solver,
        }
    }

    fn resolve_address(
        &self,
        address: &DExpr,
    ) -> Result<Option<(u64, &MemoryObject)>, MemoryError> {
        let address = self.solver.get_value(address)?;
        let address = match address.get_constant() {
            Some(address) => address,
            None => return Ok(None),
        };

        // Get the memory object with the address that is the closest below the passed address.
        for obj in self.objects.range(0..=address).rev().take(1) {
            // TODO: Perform bounds check.
            return Ok(Some((address, obj.1)));
        }

        Ok(None)
    }

    fn resolve_address_mut(
        &mut self,
        address: &DExpr,
    ) -> Result<Option<(u64, &mut MemoryObject)>, MemoryError> {
        let address = self.solver.get_value(address)?;
        let address = match address.get_constant() {
            Some(address) => address,
            None => return Ok(None),
        };

        // Get the memory object with the address that is the closest below the passed address.
        for obj in self.objects.range_mut(0..=address).rev().take(1) {
            // TODO: Perform bounds check.
            return Ok(Some((address, obj.1)));
        }

        Ok(None)
    }
}
