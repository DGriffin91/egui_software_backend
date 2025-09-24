# constify

Example usage:
```rs
#[constify]
fn foo(
    #[constify] a: bool, 
    #[constify] b: bool, 
    c: bool
) -> u32 {
    let mut sum = 0;
    if a {
        sum += 1;
    }
    if b {
        sum += 10;
    }
    if c {
        sum += 100;
    }
    sum
}
```

Expansion:
```rs
#[inline(always)]
fn foo(a: bool, b: bool, c: bool) -> u32 {
    fn foo<const a: bool, const b: bool>(c: bool) -> u32 {
        let mut sum = 0;
        if a {
            sum += 1;
        }
        if b {
            sum += 10;
        }
        if c {
            sum += 100;
        }
        sum
    }
    match (a, b) {
        (false, false) => foo::<false, false>(c),
        (true, false) => foo::<true, false>(c),
        (false, true) => foo::<false, true>(c),
        (true, true) => foo::<true, true>(c),
    }
}
```

Inspired by https://github.com/TennyZhuang/const-currying-rs