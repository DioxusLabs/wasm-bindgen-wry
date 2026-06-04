// Reserved indices - must match Rust's value.rs constants
const JSIDX_OFFSET = 128;
const JSIDX_UNDEFINED = JSIDX_OFFSET;
const JSIDX_NULL = JSIDX_OFFSET + 1;
const JSIDX_TRUE = JSIDX_OFFSET + 2;
const JSIDX_FALSE = JSIDX_OFFSET + 3;
const JSIDX_RESERVED = JSIDX_OFFSET + 4;

// Object store implementation for JS heap types.
//
// A `DeferredHeapRefs` collects values JS encoded without knowing their heap
// IDs yet. Rust allocates the IDs as it decodes the message and ships them
// back in outbound install-batch preludes. Matching is positional: install IDs
// arrive in Rust decode order, and JS installs them into the most recent
// unresolved batch first. A single JS→Rust message can be installed in chunks
// if Rust calls back into JS while it is still decoding later heap-ref args.
//
// Both sides track a batch only once it actually holds a value: Rust enqueues
// an install batch only when non-empty, and JS enqueues a `DeferredHeapRefs`
// onto the heap's stack only on its first pushed value. That keeps the two
// stacks growing in lockstep so positional matching stays exact.
class DeferredHeapRefs {
  declare private heap: JSHeap;
  declare private values: unknown[];
  declare private resolved: boolean;
  declare private enqueued: boolean;
  declare private nextInstallIndex: number;

  constructor(heap: JSHeap) {
    this.heap = heap;
    this.values = [];
    this.resolved = false;
    this.enqueued = false;
    this.nextInstallIndex = 0;
  }

  count(): number {
    return this.values.length;
  }

  push(value: unknown): void {
    if (this.resolved) {
      throw new Error("Deferred heap refs already resolved");
    }
    if (this.nextInstallIndex !== 0) {
      throw new Error("Cannot add heap refs after install has started");
    }
    // Lazily join the heap's stack on the first value so empty batches never
    // appear there — matching Rust, which only enqueues non-empty batches.
    if (!this.enqueued) {
      this.enqueued = true;
      this.heap.enqueueDeferredHeapRefs(this);
    }
    this.values.push(value);
  }

  isEmpty(): boolean {
    return this.values.length === 0;
  }

  resolve(ids: number[]): void {
    if (this.resolved) {
      throw new Error("Deferred heap refs already resolved");
    }

    const remaining = this.values.length - this.nextInstallIndex;
    if (ids.length > remaining) {
      throw new Error(
        `Heap-ref install count mismatch: ${ids.length} IDs for ${remaining} remaining values`
      );
    }

    for (let i = 0; i < ids.length; i++) {
      this.heap.insertAt(ids[i], this.values[this.nextInstallIndex + i]);
    }
    this.nextInstallIndex += ids.length;
    this.resolved = this.nextInstallIndex === this.values.length;
  }

  isResolved(): boolean {
    return this.resolved;
  }
}

class JSHeap {
  declare private slots: Map<number, unknown>;
  declare private heapObjectCount: number;
  // Borrow stack uses indices 1-127, growing downward from 127 to 1
  declare private borrowStackPointer: number;
  // Frame stack for nested operations - saves borrow stack pointers
  declare private borrowFrameStack: number[];
  // Stack of reservation scopes: each scope tracks exact IDs reserved by Rust
  declare private reservationStack: { ids: number[]; nextIndex: number }[];
  // Stack of DeferredHeapRefs awaiting Rust-allocated install IDs. Rust ships
  // batches in decode order; the most recently created DHR is at the top and
  // is the one being installed by the next batch in an inbound prelude.
  declare private deferredHeapRefs: DeferredHeapRefs[];

  constructor() {
    // Slots 0-127 are for borrow stack (1-127 usable), slots 128-131
    // are reserved for special values.
    // A Map avoids sparse array slowdowns as Rust assigns high heap IDs.
    this.slots = new Map();

    this.slots.set(JSIDX_NULL, null);
    this.slots.set(JSIDX_TRUE, true);
    this.slots.set(JSIDX_FALSE, false);
    this.slots.set(JSIDX_UNDEFINED, undefined);

    this.heapObjectCount = 0;
    // Borrow stack pointer starts at 128 (just below reserved values)
    this.borrowStackPointer = JSIDX_OFFSET;
    // Frame stack starts empty
    this.borrowFrameStack = [];
    // Reservation stack starts empty
    this.reservationStack = [];
    this.deferredHeapRefs = [];
  }

  insertAt(id: number, value: unknown): void {
    if (id < JSIDX_RESERVED) {
      throw new Error(`Cannot install heap ref into special slot ${id}`);
    }
    if (id >= JSIDX_RESERVED && !this.slots.has(id)) {
      this.heapObjectCount++;
    }
    this.slots.set(id, value);
  }

  // Create a batch for an outbound message. It joins the stack lazily, only
  // once it collects its first value (see DeferredHeapRefs.push).
  deferHeapRefs(): DeferredHeapRefs {
    return new DeferredHeapRefs(this);
  }

  // Push a non-empty batch onto the stack. Called by DeferredHeapRefs on its
  // first value so empty batches never occupy a stack slot.
  enqueueDeferredHeapRefs(refs: DeferredHeapRefs): void {
    this.deferredHeapRefs.push(refs);
  }

  // Apply the next install batch from an inbound prelude to the most
  // recently created unresolved DeferredHeapRefs.
  resolveDeferredHeapRefs(ids: number[]): void {
    const refs = this.deferredHeapRefs[this.deferredHeapRefs.length - 1];
    if (!refs) {
      throw new Error(
        "Received an install batch but no deferred heap-ref frame is pending"
      );
    }
    refs.resolve(ids);
    if (refs.isResolved()) {
      this.deferredHeapRefs.pop();
    }
  }

  // Push a reservation scope for exact IDs allocated by Rust
  pushReservationScope(ids: number[]): void {
    this.reservationStack.push({ ids, nextIndex: 0 });
    for (const id of ids) {
      if (id < JSIDX_RESERVED) {
        throw new Error(`Cannot reserve special heap slot ${id}`);
      }
      if (this.slots.has(id)) {
        throw new Error(`Reserved heap slot ${id} is already occupied`);
      }
    }
  }

  // Pop the current reservation scope. `validate` defaults to true and asserts
  // every reserved slot was filled. Callers pass false when the op loop aborted
  // early (e.g. a decode error) so this cleanup does not throw over and mask the
  // original error; the stack is still popped to stay balanced.
  popReservationScope(validate = true): void {
    const scope = this.reservationStack.pop();
    if (validate && scope && scope.nextIndex !== scope.ids.length) {
      throw new Error(
        `Only filled ${scope.nextIndex} of ${scope.ids.length} reserved heap slots`
      );
    }
  }

  // Fill the next reserved slot in the current scope
  fillNextReserved(value: unknown): void {
    const scope = this.reservationStack[this.reservationStack.length - 1];
    if (!scope || scope.nextIndex >= scope.ids.length) {
      throw new Error("No reserved slots available");
    }
    const id = scope.ids[scope.nextIndex];
    scope.nextIndex++;
    this.insertAt(id, value);
  }

  get(id: number): unknown | undefined {
    return this.slots.get(id);
  }

  remove(id: number): unknown | undefined {
    // Never remove reserved slots
    if (id < JSIDX_RESERVED) {
      return this.slots.get(id);
    }

    if (!this.has(id)) {
      return undefined;
    }

    const value = this.slots.get(id);

    this.slots.delete(id);
    this.heapObjectCount--;
    return value;
  }

  has(id: number): boolean {
    return this.slots.has(id);
  }

  heapObjectsAlive(): number {
    return this.heapObjectCount;
  }

  // Add a borrowed reference to the borrow stack (indices 1-127)
  // Returns the stack slot index
  addBorrowedRef(obj: unknown): number {
    if (this.borrowStackPointer <= 1) {
      throw new Error(
        "Borrow stack overflow: too many borrowed references in a single operation"
      );
    }
    this.borrowStackPointer--;
    this.slots.set(this.borrowStackPointer, obj);
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
        this.slots.delete(i);
      }
      this.borrowStackPointer = savedPointer;
    }
  }
}

export { DeferredHeapRefs, JSHeap };
