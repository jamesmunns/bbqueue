# BBQueue Storage Work

## Use cases

Current: "Embedded Use Case"

* Statically allocate storage
* BBBuffer lives forever
* User uses Producer and Consumer

Future:

* Have the STORAGE for BBBuffer be provided seperately
* Allow for uses like:
    * Statically allocated storage (like now)
    * Heap Allocation provided storage (Arc, etc.)
    * User provided storage (probably `unsafe`)

## Sample Code for Use Cases

### Static Buffer

```rust
static BB_QUEUE: BBQueue<BBBuffer<1024>> = BBQueue::new(BBBuffer::new());

fn main() {
    let (prod, cons) = BB_QUEUE.try_split().unwrap();
    // ...
}
```

### Heap Allocation provided storage

Choice A: Simple

```rust
fn main() {
    // BBQueue<Arc<BBBuffer<1024>>
    // Producer<Arc<BBBuffer<1024>>, Consumer<Arc<BBBuffer<1024>>
    //
    // Storage is dropped when `prod` and `cons` are BOTH dropped.
    let (prod, cons) = BBQueue::new_arc::<1024>();
}
```

Choice B: Explicit

```rust
fn main() {
    // EDIT: This is sub-par, because this would require `arc_queue`,
    // `prod`, and `storage` to ALL be dropped
    // before the buffer is dropped.
    let arc_queue: BBQueue<Arc<BBBuffer<1024>> = BBStorage::new_arc();
    let (prod, cons) = arc_queue.try_split().unwrap();
}
```

### User provided storage

Choice A: Naive

EDIT: Not this. See below

```rust
static mut UNSAFE_BUFFER: [u8; 1024] = [0u8; 1024];

fn main() {
    let borrowed = unsafe {
        // TODO: Make sure BBQueue has lifetime shorter
        // than `'borrowed` here? In this case it is
        // 'static, but may not always be.
        BBStorageBorrowed::new(&mut UNSAFE_BUFFER);
    };
    let bbqueue = BBQueue::new(borrowed);

    // NOTE: This is NOT good, because the bound lifetime
    // of prod and cons will be that of `bbqueue`, which
    // is probably not suitable (non-'static). In many cases, we want
    // the producer and consumer to also have `Producer<'static>` lifetime
    let (prod, cons) = bbqueue.try_split().unwrap();
}
```

Choice B: "loadable" storage?

This would require EITHER:

* The BBStorage methods are failable
* The split belongs to the BBStorage item
    * (Could be an inherent or trait method)
* Loadable storage panics on a split if not loaded

```rust
static mut UNSAFE_BUFFER: [u8; 1024] = [0u8; 1024];
static LOADABLE_BORROWED: BBStorageLoadBorrow::new();

fn main() {
    // This could probably be shortened to a single "store and take header" action.
    // Done in multiple steps here for clarity.
    let mut_buf = unsafe {
        &mut UNSAFE_BUFFER
    };
    let old = LOADABLE_BORROWED.store();    // -> Result<Option<&mut [u8]>>
                                            // Result: Err if already taken
                                            // Option: Some if other buffer already stored
    assert_eq!(Ok(None), old);

    let bbqueue = BBQueue::new(LOADABLE_BORROWED.take_header().unwrap());

    // Here prod and cons are <'static>, because LOADABLE_BORROWED is static.
    // BUUUUUT we still probably allow access of BBStorage methods, which would be totally unsafe
    //
    // EDIT: Okay, sealing the trait DOES prevent outer usage, so we're good on this regard!
    let (prod, cons) = bbqueue.try_split().unwrap();
}
```
