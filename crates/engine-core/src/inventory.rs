use crate::BlockId;

/// An item stack stored in an inventory slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    pub block_id: BlockId,
    pub count: u32,
}

impl ItemStack {
    pub const MAX_STACK: u32 = 64;

    pub fn new(block_id: BlockId, count: u32) -> Self {
        Self {
            block_id,
            count: count.min(Self::MAX_STACK),
        }
    }
}

pub const HOTBAR_SLOTS: usize = 9;
pub const BACKPACK_ROWS: usize = 3;
pub const BACKPACK_COLS: usize = 9;
pub const BACKPACK_SLOTS: usize = BACKPACK_ROWS * BACKPACK_COLS; // 27
pub const TOTAL_INVENTORY_SLOTS: usize = HOTBAR_SLOTS + BACKPACK_SLOTS; // 36

/// Player inventory consisting of 9 hotbar slots and 27 backpack slots.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub slots: [Option<ItemStack>; TOTAL_INVENTORY_SLOTS],
    pub selected_hotbar: usize, // 0..9
}

impl Default for Inventory {
    fn default() -> Self {
        Self::new()
    }
}

impl Inventory {
    pub fn new() -> Self {
        Self {
            slots: [None; TOTAL_INVENTORY_SLOTS],
            selected_hotbar: 0,
        }
    }

    /// Creates an inventory pre-filled with starter survival blocks.
    pub fn with_starter_blocks(blocks: &[BlockId]) -> Self {
        let mut inv = Self::new();
        for (i, &b) in blocks.iter().take(HOTBAR_SLOTS).enumerate() {
            inv.slots[i] = Some(ItemStack::new(b, 32));
        }
        inv
    }

    pub fn selected_item(&self) -> Option<ItemStack> {
        self.slots.get(self.selected_hotbar).copied().flatten()
    }

    pub fn selected_block(&self) -> BlockId {
        self.selected_item().map(|s| s.block_id).unwrap_or(BlockId::AIR)
    }

    pub fn set_selected_slot(&mut self, slot: usize) {
        if slot < HOTBAR_SLOTS {
            self.selected_hotbar = slot;
        }
    }

    /// Adds items into the inventory, stacking with matching blocks first, then filling empty slots.
    /// Returns any excess amount that could not fit.
    pub fn add_item(&mut self, block_id: BlockId, mut amount: u32) -> u32 {
        if block_id == BlockId::AIR || amount == 0 {
            return amount;
        }

        // 1. Try to merge into existing non-full stacks
        for slot in self.slots.iter_mut() {
            if let Some(stack) = slot {
                if stack.block_id == block_id && stack.count < ItemStack::MAX_STACK {
                    let space = ItemStack::MAX_STACK - stack.count;
                    let to_add = amount.min(space);
                    stack.count += to_add;
                    amount -= to_add;
                    if amount == 0 {
                        return 0;
                    }
                }
            }
        }

        // 2. Put into first empty slot
        for slot in self.slots.iter_mut() {
            if slot.is_none() {
                let to_add = amount.min(ItemStack::MAX_STACK);
                *slot = Some(ItemStack::new(block_id, to_add));
                amount -= to_add;
                if amount == 0 {
                    return 0;
                }
            }
        }

        amount
    }

    /// Deducts `amount` from the currently selected hotbar slot.
    /// Returns true if items were consumed, false if insufficient items.
    pub fn consume_selected(&mut self, amount: u32) -> bool {
        if let Some(stack) = &mut self.slots[self.selected_hotbar] {
            if stack.count >= amount {
                stack.count -= amount;
                if stack.count == 0 {
                    self.slots[self.selected_hotbar] = None;
                }
                return true;
            }
        }
        false
    }

    /// Swaps or merges slot `a` and slot `b`.
    pub fn swap_or_merge(&mut self, a: usize, b: usize) {
        if a >= TOTAL_INVENTORY_SLOTS || b >= TOTAL_INVENTORY_SLOTS || a == b {
            return;
        }
        match (self.slots[a], self.slots[b]) {
            (Some(stack_a), Some(mut stack_b)) if stack_a.block_id == stack_b.block_id => {
                let space = ItemStack::MAX_STACK - stack_b.count;
                let to_move = stack_a.count.min(space);
                stack_b.count += to_move;
                self.slots[b] = Some(stack_b);
                let rem_a = stack_a.count - to_move;
                if rem_a > 0 {
                    self.slots[a] = Some(ItemStack {
                        block_id: stack_a.block_id,
                        count: rem_a,
                    });
                } else {
                    self.slots[a] = None;
                }
            }
            _ => {
                self.slots.swap(a, b);
            }
        }
    }
}
