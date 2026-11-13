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

/// An item stored inside a spatial 2D grid inventory with position, dimensions, rotation, and weight.
#[derive(Debug, Clone, PartialEq)]
pub struct GridItem {
    pub stack: ItemStack,
    /// Top-left grid column coordinate (0..width).
    pub x: usize,
    /// Top-left grid row coordinate (0..height).
    pub y: usize,
    /// Base cell dimensions `(width, height)` in unrotated state.
    pub size: (u8, u8),
    /// Whether the item is rotated 90 degrees (swapping width and height).
    pub rotated: bool,
    /// Unit weight per single item in kilograms.
    pub unit_weight: f32,
}

impl GridItem {
    pub fn new(stack: ItemStack, x: usize, y: usize, size: (u8, u8), unit_weight: f32) -> Self {
        Self {
            stack,
            x,
            y,
            size: (size.0.max(1), size.1.max(1)),
            rotated: false,
            unit_weight: unit_weight.max(0.0),
        }
    }

    /// Effective dimensions on the grid, accounting for 90-degree rotation.
    #[inline]
    pub fn effective_size(&self) -> (usize, usize) {
        if self.rotated {
            (self.size.1 as usize, self.size.0 as usize)
        } else {
            (self.size.0 as usize, self.size.1 as usize)
        }
    }

    /// Total weight of this entire stack in kilograms.
    #[inline]
    pub fn total_weight(&self) -> f32 {
        self.unit_weight * self.stack.count as f32
    }
}

/// Universal spatial 2D grid inventory container with variable item sizes, rotation, and weight limits
/// designed for hardcore survival (DayZ, Project Zomboid, Escape from Tarkov, Resident Evil).
#[derive(Debug, Clone, PartialEq)]
pub struct GridContainer {
    pub width: usize,
    pub height: usize,
    /// Maximum allowed weight in kilograms (0.0 = unlimited capacity).
    pub max_weight: f32,
    pub items: Vec<GridItem>,
}

impl GridContainer {
    pub fn new(width: usize, height: usize, max_weight: f32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
            max_weight: max_weight.max(0.0),
            items: Vec::new(),
        }
    }

    /// Total weight of all items currently stored in the container.
    pub fn total_weight(&self) -> f32 {
        self.items.iter().map(|item| item.total_weight()).sum()
    }

    /// Checks if adding additional weight would exceed the container's carrying capacity.
    pub fn can_carry(&self, additional_weight: f32) -> bool {
        if self.max_weight <= 0.0 {
            return true;
        }
        self.total_weight() + additional_weight <= self.max_weight + 0.0001
    }

    /// Checks whether a specific grid cell is occupied by any item (optionally ignoring an item index).
    pub fn is_cell_occupied(&self, x: usize, y: usize, exclude_index: Option<usize>) -> bool {
        if x >= self.width || y >= self.height {
            return true;
        }
        for (i, item) in self.items.iter().enumerate() {
            if Some(i) == exclude_index {
                continue;
            }
            let (w, h) = item.effective_size();
            if x >= item.x && x < item.x + w && y >= item.y && y < item.y + h {
                return true;
            }
        }
        false
    }

    /// Verifies if an item can legally be placed at its coordinates without collisions or out-of-bounds.
    pub fn can_place_item(&self, item: &GridItem, exclude_index: Option<usize>) -> bool {
        let (w, h) = item.effective_size();
        if item.x + w > self.width || item.y + h > self.height {
            return false;
        }

        for (i, other) in self.items.iter().enumerate() {
            if Some(i) == exclude_index {
                continue;
            }
            let (ow, oh) = other.effective_size();
            let overlap_x = item.x < other.x + ow && item.x + w > other.x;
            let overlap_y = item.y < other.y + oh && item.y + h > other.y;
            if overlap_x && overlap_y {
                return false;
            }
        }

        true
    }

    /// Inserts an item into the grid at its specified position.
    pub fn insert(&mut self, item: GridItem) -> Result<usize, &'static str> {
        if !self.can_carry(item.total_weight()) {
            return Err("Перегруз: превышен лимит веса контейнера");
        }
        if !self.can_place_item(&item, None) {
            return Err("Коллизия: позиция занята другим предметом или выходит за границы сетки");
        }
        let idx = self.items.len();
        self.items.push(item);
        Ok(idx)
    }

    /// Automatically searches for the first available space (trying regular, then rotated orientation)
    /// to place a new item stack.
    pub fn auto_pack(
        &mut self,
        stack: ItemStack,
        size: (u8, u8),
        unit_weight: f32,
    ) -> Result<usize, &'static str> {
        let weight = unit_weight.max(0.0) * stack.count as f32;
        if !self.can_carry(weight) {
            return Err("Перегруз: превышен лимит веса контейнера");
        }

        let mut candidate = GridItem::new(stack, 0, 0, size, unit_weight);

        // Try regular orientation first (row by row)
        for y in 0..self.height {
            for x in 0..self.width {
                candidate.x = x;
                candidate.y = y;
                candidate.rotated = false;
                if self.can_place_item(&candidate, None) {
                    let idx = self.items.len();
                    self.items.push(candidate);
                    return Ok(idx);
                }
            }
        }

        // Try rotated 90 degrees if width != height
        if size.0 != size.1 {
            for y in 0..self.height {
                for x in 0..self.width {
                    candidate.x = x;
                    candidate.y = y;
                    candidate.rotated = true;
                    if self.can_place_item(&candidate, None) {
                        let idx = self.items.len();
                        self.items.push(candidate);
                        return Ok(idx);
                    }
                }
            }
        }

        Err("Недостаточно свободного места в сетке для размещения предмета")
    }

    /// Removes an item at the given index.
    pub fn remove(&mut self, index: usize) -> Option<GridItem> {
        if index < self.items.len() {
            Some(self.items.remove(index))
        } else {
            None
        }
    }

    /// Retrieves an immutable reference to the item covering cell `(x, y)` along with its index.
    pub fn item_at(&self, x: usize, y: usize) -> Option<(usize, &GridItem)> {
        for (i, item) in self.items.iter().enumerate() {
            let (w, h) = item.effective_size();
            if x >= item.x && x < item.x + w && y >= item.y && y < item.y + h {
                return Some((i, item));
            }
        }
        None
    }

    /// Attempts to rotate an item by 90 degrees in-place.
    pub fn rotate_item(&mut self, index: usize) -> Result<(), &'static str> {
        if index >= self.items.len() {
            return Err("Предмет не найден");
        }
        let mut test_item = self.items[index].clone();
        test_item.rotated = !test_item.rotated;

        if self.can_place_item(&test_item, Some(index)) {
            self.items[index].rotated = test_item.rotated;
            Ok(())
        } else {
            Err("Невозможно повернуть: предмет перекрывает другие вещи или выходит за границы")
        }
    }

    /// Moves an item to a new coordinate within the grid.
    pub fn move_item(&mut self, index: usize, new_x: usize, new_y: usize) -> Result<(), &'static str> {
        if index >= self.items.len() {
            return Err("Предмет не найден");
        }
        let mut test_item = self.items[index].clone();
        test_item.x = new_x;
        test_item.y = new_y;

        if self.can_place_item(&test_item, Some(index)) {
            self.items[index].x = new_x;
            self.items[index].y = new_y;
            Ok(())
        } else {
            Err("Целевая позиция занята или выходит за пределы сетки")
        }
    }
}

/// Player inventory consisting of 9 hotbar slots, 27 backpack slots, and an optional grid container mode.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub slots: [Option<ItemStack>; TOTAL_INVENTORY_SLOTS],
    pub selected_hotbar: usize, // 0..9
    /// Optional Tarkov/DayZ-style spatial grid container for advanced inventory modes.
    pub grid_mode: Option<GridContainer>,
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
            grid_mode: None,
        }
    }

    /// Creates an inventory with grid container enabled.
    pub fn with_grid(width: usize, height: usize, max_weight: f32) -> Self {
        Self {
            slots: [None; TOTAL_INVENTORY_SLOTS],
            selected_hotbar: 0,
            grid_mode: Some(GridContainer::new(width, height, max_weight)),
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
