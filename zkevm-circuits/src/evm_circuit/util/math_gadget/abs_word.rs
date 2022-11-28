use crate::{
    evm_circuit::util::{
        self, constraint_builder::ConstraintBuilder, from_bytes, math_gadget::*, CachedRegion,
    },
    util::Expr,
};
use eth_types::{Field, ToLittleEndian, Word};
use halo2_proofs::plonk::Error;
/// Construction of 256-bit word original and absolute values, which is useful
/// for opcodes operated on signed values.
#[derive(Clone, Debug)]
pub(crate) struct AbsWordGadget<F> {
    x: util::Word<F>,
    x_abs: util::Word<F>,
    sum: util::Word<F>,
    is_neg: LtGadget<F, 1>,
    add_words: AddWordsGadget<F, 2, false>,
}

impl<F: Field> AbsWordGadget<F> {
    pub(crate) fn construct(cb: &mut ConstraintBuilder<F>) -> Self {
        let x = cb.query_word();
        let x_abs = cb.query_word();
        let sum = cb.query_word();
        let x_lo = from_bytes::expr(&x.cells[0..16]);
        let x_hi = from_bytes::expr(&x.cells[16..32]);
        let x_abs_lo = from_bytes::expr(&x_abs.cells[0..16]);
        let x_abs_hi = from_bytes::expr(&x_abs.cells[16..32]);
        let is_neg = LtGadget::construct(cb, 127.expr(), x.cells[31].expr());

        cb.add_constraint(
            "x_abs_lo == x_lo when x >= 0",
            (1.expr() - is_neg.expr()) * (x_abs_lo.expr() - x_lo.expr()),
        );
        cb.add_constraint(
            "x_abs_hi == x_hi when x >= 0",
            (1.expr() - is_neg.expr()) * (x_abs_hi.expr() - x_hi.expr()),
        );

        // When `is_neg`, constrain `sum == 0` and `carry == 1`. Since the final
        // result is `1 << 256`.
        let add_words = AddWordsGadget::construct(cb, [x.clone(), x_abs.clone()], sum.clone());
        cb.add_constraint(
            "sum == 0 when x < 0",
            is_neg.expr() * add_words.sum().expr(),
        );
        cb.add_constraint(
            "carry_hi == 1 when x < 0",
            is_neg.expr() * (1.expr() - add_words.carry().as_ref().unwrap().expr()),
        );

        Self {
            x,
            x_abs,
            sum,
            is_neg,
            add_words,
        }
    }

    pub(crate) fn assign(
        &self,
        region: &mut CachedRegion<'_, '_, F>,
        offset: usize,
        x: Word,
        x_abs: Word,
    ) -> Result<(), Error> {
        self.x.assign(region, offset, Some(x.to_le_bytes()))?;
        self.x_abs
            .assign(region, offset, Some(x_abs.to_le_bytes()))?;
        self.is_neg.assign(
            region,
            offset,
            127.into(),
            u64::from(x.to_le_bytes()[31]).into(),
        )?;
        let sum = x.overflowing_add(x_abs).0;
        self.sum.assign(region, offset, Some(sum.to_le_bytes()))?;
        self.add_words.assign(region, offset, [x, x_abs], sum)
    }

    pub(crate) fn x(&self) -> &util::Word<F> {
        &self.x
    }

    pub(crate) fn x_abs(&self) -> &util::Word<F> {
        &self.x_abs
    }

    pub(crate) fn is_neg(&self) -> &LtGadget<F, 1> {
        &self.is_neg
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_util::*;
    use super::*;
    use eth_types::{Word, U256};
    use halo2_proofs::halo2curves::bn256::Fr;
    use halo2_proofs::plonk::Error;

    #[derive(Clone)]
    struct AbsWordGadgetContainer<F, const IS_NEG: bool> {
        absword_gadget: AbsWordGadget<F>,
    }

    impl<F: Field, const IS_NEG: bool> MathGadgetContainer<F> for AbsWordGadgetContainer<F, IS_NEG> {
        const NAME: &'static str = "AbsWordGadget";

        fn configure_gadget_container(cb: &mut ConstraintBuilder<F>) -> Self {
            let absword_gadget = AbsWordGadget::<F>::construct(cb);
            cb.require_equal(
                "is_neg is correct",
                absword_gadget.is_neg().expr(),
                IS_NEG.expr(),
            );
            AbsWordGadgetContainer { absword_gadget }
        }

        fn assign_gadget_container(
            &self,
            witness_words: &[Word],
            region: &mut CachedRegion<'_, '_, F>,
        ) -> Result<(), Error> {
            let offset = 0;
            let x = witness_words[0];
            let x_abs = witness_words[1];
            self.absword_gadget.assign(region, offset, x, x_abs)?;

            Ok(())
        }
    }

    macro_rules! try_test_abs_x {
        ($x:expr; is neg, abs eq $x_abs:expr, $expect_success:expr) => {{
            try_test!(AbsWordGadgetContainer<Fr, true>, [$x, $x_abs], $expect_success)
        }};
        ($x:expr; is not neg, abs eq $x_abs:expr, $expect_success:expr) => {{
            try_test!(AbsWordGadgetContainer<Fr, false>, [$x, $x_abs], $expect_success)
        }};
    }

    #[test]
    fn test_abs_0_eq_0() {
        try_test_abs_x!(Word::from(0); is not neg, abs eq Word::from(0), true);
    }

    #[test]
    fn test_abs_1_eq_1() {
        try_test_abs_x!(Word::from(1); is not neg, abs eq Word::from(1), true);
    }

    #[test]
    fn test_abs_1_neq_2() {
        try_test_abs_x!(Word::from(1); is not neg, abs eq Word::from(2), false);
    }

    #[test]
    fn test_abs_wordmax_eq_minus1() {
        try_test_abs_x!(Word::MAX; is neg, abs eq Word::from(1), true);
    }

    #[test]
    fn test_abs_word_high_max() {
        let abs_high_max = U256([0, 0, 1, 0]);
        try_test_abs_x!(WORD_HIGH_MAX; is neg, abs eq abs_high_max, true);
    }

    #[test]
    fn test_abs_word_low_max() {
        try_test_abs_x!(WORD_LOW_MAX; is not neg, abs eq WORD_LOW_MAX, true);
    }

    #[test]
    fn test_abs_unexpected_is_neg_for_postive() {
        try_test_abs_x!(WORD_LOW_MAX; is neg, abs eq WORD_LOW_MAX, false);
    }

    #[test]
    fn test_abs_unexpected_is_neg_for_negtive() {
        try_test_abs_x!(Word::MAX; is not neg, abs eq Word::from(1), false);
    }
}
