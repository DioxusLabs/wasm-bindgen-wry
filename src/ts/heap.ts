// Reserved indices - must match Rust's value.rs constants
const JSIDX_OFFSET = 128;
const JSIDX_UNDEFINED = JSIDX_OFFSET;
const JSIDX_NULL = JSIDX_OFFSET + 1;
const JSIDX_TRUE = JSIDX_OFFSET + 2;
const JSIDX_FALSE = JSIDX_OFFSET + 3;
const JSIDX_RESERVED = JSIDX_OFFSET + 4;

// SlotMap implementation for JS heap types
class JSHeap {
  private slots: (unknown | undefined)[];
  private freeIds: number[];
  private maxId: number;

  constructor() {
    // Pre-allocate slots array - slots 0-127 are unused gaps,
    // slots 128-131 are reserved for special values (undefined, null, true, false),
    // heap allocation starts at 132 (JSIDX_RESERVED)
    this.slots = [];

    this.slots[JSIDX_NULL] = null;
    this.slots[JSIDX_TRUE] = true;
    this.slots[JSIDX_FALSE] = false;
    this.slots[JSIDX_UNDEFINED] = undefined;

    this.freeIds = [];
    // Start allocating from JSIDX_RESERVED (132)
    this.maxId = JSIDX_RESERVED;
  }

  insert(value: unknown): number {
    let id: number;
    if (this.freeIds.length > 0) {
      id = this.freeIds.pop()!;
      console.log("[JS Heap] insert (reused id):", id, "value:", value);
    } else {
      id = this.maxId;
      this.maxId++;
      console.log("[JS Heap] insert (new id):", id, "value:", value);
    }
    this.slots[id] = value;
    return id;
  }

  get(id: number): unknown | undefined {
    console.log("[JS Heap] get:", id, "value:", this.slots[id]);
    return this.slots[id];
  }

  remove(id: number): unknown | undefined {
    // Never remove reserved slots
    if (id < JSIDX_RESERVED) {
      console.log("[JS Heap] remove (reserved, skipping):", id);
      return this.slots[id];
    }

    const value = this.slots[id];
    if (value !== undefined) {
      console.log("[JS Heap] remove:", id, "value:", value);
      this.slots[id] = undefined;
      this.freeIds.push(id);
    } else {
      console.log("[JS Heap] remove (already undefined):", id);
    }
    return value;
  }

  has(id: number): boolean {
    return this.slots[id] !== undefined;
  }
}

export { JSHeap };