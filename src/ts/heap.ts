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
  // Borrow stack uses indices 1-127, growing downward from 127 to 1
  private borrowStackPointer: number;
  // Frame stack for nested operations - saves borrow stack pointers
  private borrowFrameStack: number[];

  constructor() {
    // Pre-allocate slots array - slots 0-127 are for borrow stack (1-127 usable),
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
    // Borrow stack pointer starts at 128 (just below reserved values)
    this.borrowStackPointer = JSIDX_OFFSET;
    // Frame stack starts empty
    this.borrowFrameStack = [];
  }

  insert(value: unknown): number {
    let id: number;
    if (this.freeIds.length > 0) {
      id = this.freeIds.pop()!;
    } else {
      id = this.maxId;
      this.maxId++;
    }
    this.slots[id] = value;
    return id;
  }

  get(id: number): unknown | undefined {
    return this.slots[id];
  }

  remove(id: number): unknown | undefined {
    // Never remove reserved slots
    if (id < JSIDX_RESERVED) {
      return this.slots[id];
    }

    const value = this.slots[id];

    this.slots[id] = undefined;
    this.freeIds.push(id);
    return value;
  }

  has(id: number): boolean {
    return this.freeIds.indexOf(id) === -1 && id < this.slots.length;
  }

  heapObjectsAlive(): number {
    return this.slots.length - this.freeIds.length - JSIDX_RESERVED;
  }

  // Add a borrowed reference to the borrow stack (indices 1-127)
  // Returns the stack slot index
  addBorrowedRef(obj: unknown): number {
    if (this.borrowStackPointer <= 1) {
      throw new Error("Borrow stack overflow: too many borrowed references in a single operation");
    }
    this.borrowStackPointer--;
    this.slots[this.borrowStackPointer] = obj;
    return this.borrowStackPointer;
  }

  // Push a borrow frame before a nested operation that may add borrowed refs
  // This saves the current borrow stack pointer so we can restore it later
  pushBorrowFrame(): void {
    this.borrowFrameStack.push(this.borrowStackPointer);
  }

  // Pop a borrow frame after a nested operation completes
  // This clears only the borrowed refs from this frame and restores the pointer
  popBorrowFrame(): void {
    const savedPointer = this.borrowFrameStack.pop();
    if (savedPointer !== undefined) {
      // Clear refs from this frame only (from current pointer up to saved pointer)
      for (let i = this.borrowStackPointer; i < savedPointer; i++) {
        this.slots[i] = undefined;
      }
      this.borrowStackPointer = savedPointer;
    }
  }

  // Get the current borrow stack pointer (for testing)
  getBorrowStackPointer(): number {
    return this.borrowStackPointer;
  }
}

export { JSHeap };
