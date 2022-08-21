# awwoc

The most uwu awwocator ewer.

## awwoc is bwazing fast

here's how awwoc compawes to the default wust awwocator, in a very accuwate benchmawk:
1. allocate 10000 10 byte allocations, writing `69` to each byte.
2. deallocate all of them

| awwocator    | time      |
|--------------|-----------|
| wust default | 2.65 ms   |
| awwoc        | 763.73 ms |

as you can see, awwoc handily beats the wust awwocator.

## using awwoc

add
```rs
#[global_allocator]
static AWWOC: awwoc::Awwoc = awwoc::Awwoc;
```
to your wust pwogwam.

## unwuckily

awwoc only works on GNU/Winyux, or other posix opewating system kewnels.
it also works on every platform using miwi!
