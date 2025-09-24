mod tests {
    use constify::constify;
    #[constify]
    fn foo(#[constify] a: bool, #[constify] b: bool, c: bool) -> u32 {
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

    #[test]
    fn simple() {
        assert_eq!(foo(false, false, false), 0);
        assert_eq!(foo(true, false, false), 1);
        assert_eq!(foo(false, true, false), 10);
        assert_eq!(foo(false, false, true), 100);
        assert_eq!(foo(true, true, false), 11);
        assert_eq!(foo(true, true, true), 111);
        assert_eq!(foo(true, false, true), 101);
        assert_eq!(foo(false, true, true), 110);
    }
}
