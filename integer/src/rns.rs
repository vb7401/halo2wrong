use crate::{WrongExt, NUMBER_OF_LOOKUP_LIMBS};
use halo2::arithmetic::FieldExt;
use maingate::{big_to_fe, compose, decompose_big, fe_to_big, halo2};
use num_bigint::BigUint as big_uint;
use num_integer::Integer as _;
use num_traits::{Num, One, Zero};
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;

#[cfg(feature = "kzg")]
use crate::halo2::arithmetic::BaseExt;

pub trait Common<F: FieldExt> {
    fn value(&self) -> big_uint;

    fn native(&self) -> F {
        let native_value = self.value() % modulus::<F>();
        big_to_fe(native_value)
    }

    fn eq(&self, other: &Self) -> bool {
        self.value() == other.value()
    }
}

cfg_if::cfg_if! {
    if #[cfg(feature = "kzg")] {
        fn modulus<F: BaseExt>() -> big_uint {
            big_uint::from_str_radix(&F::MODULUS[2..], 16).unwrap()
        }

    } else {
        // default feature
        fn modulus<F: FieldExt>() -> big_uint {
            big_uint::from_str_radix(&F::MODULUS[2..], 16).unwrap()
        }
    }
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    From<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>> for big_uint
{
    fn from(el: Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>) -> Self {
        el.value()
    }
}

fn bool_to_big(truth: bool) -> big_uint {
    if truth {
        big_uint::one()
    } else {
        big_uint::zero()
    }
}

impl<F: FieldExt> From<Limb<F>> for big_uint {
    fn from(limb: Limb<F>) -> Self {
        limb.value()
    }
}

// Reduction witness contains all values that needs to be assigned in
// multiplication gate.
#[derive(Clone)]
pub(crate) struct ReductionWitness<
    W: WrongExt,
    N: FieldExt,
    const NUMBER_OF_LIMBS: usize,
    const BIT_LEN_LIMB: usize,
> {
    pub(crate) result: Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>,
    quotient: Quotient<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>,
    intermediate: [N; NUMBER_OF_LIMBS],
    residues: Vec<N>,
}

// Wrapper for reduction witnesses
pub(crate) struct MaybeReduced<
    W: WrongExt,
    N: FieldExt,
    const NUMBER_OF_LIMBS: usize,
    const BIT_LEN_LIMB: usize,
>(Option<ReductionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>);

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    From<Option<ReductionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>>
    for MaybeReduced<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    fn from(integer: Option<ReductionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        MaybeReduced(integer)
    }
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    MaybeReduced<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    // Expect an integer quotient
    pub(crate) fn long(&self) -> Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>> {
        self.0
            .as_ref()
            .map(|reduction_result| match reduction_result.quotient.clone() {
                Quotient::Long(quotient) => quotient,
                _ => panic!("long quotient expected"),
            })
    }

    // Expect a limb quotient
    pub(crate) fn short(&self) -> Option<N> {
        self.0
            .as_ref()
            .map(|reduction_result| match reduction_result.quotient.clone() {
                Quotient::Short(quotient) => quotient,
                _ => panic!("short quotient expected"),
            })
    }

    pub(crate) fn result(&self) -> Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>> {
        self.0.as_ref().map(|u| u.result.clone())
    }

    pub(crate) fn residues(&self) -> Vec<Option<N>> {
        let u_len = (NUMBER_OF_LIMBS + 1) / 2;
        (0..u_len)
            .map(|i| {
                match &self.0 {
                    Some(witness) => Some(witness.residues[i]),
                    None => None,
                }
                .into()
            })
            .collect()
    }

    pub(crate) fn intermediates(&self) -> Vec<Option<N>> {
        (0..NUMBER_OF_LIMBS)
            .map(|i| {
                match &self.0 {
                    Some(witness) => Some(witness.intermediate[i]),
                    None => None,
                }
                .into()
            })
            .collect()
    }
}

#[derive(Clone)]
pub enum Quotient<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
{
    Short(N),
    Long(Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>),
}

// Comparision witnesses contains all values that needs to be assigned in
// comparision gate.
#[derive(Clone)]
pub(crate) struct ComparisionWitness<
    W: WrongExt,
    N: FieldExt,
    const NUMBER_OF_LIMBS: usize,
    const BIT_LEN_LIMB: usize,
> {
    pub result: Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>,
    pub borrow: [bool; NUMBER_OF_LIMBS],
}

#[derive(Debug, Clone)]
pub struct Rns<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize> {
    pub bit_len_lookup: usize,

    pub wrong_modulus: big_uint,
    pub native_modulus: big_uint,
    pub binary_modulus: big_uint,
    pub crt_modulus: big_uint,

    pub(crate) right_shifters: [N; NUMBER_OF_LIMBS],
    pub(crate) left_shifters: [N; NUMBER_OF_LIMBS],

    pub base_aux: [big_uint; NUMBER_OF_LIMBS],

    pub negative_wrong_modulus_decomposed: [N; NUMBER_OF_LIMBS],
    pub wrong_modulus_decomposed: [N; NUMBER_OF_LIMBS],
    pub wrong_modulus_minus_one: [N; NUMBER_OF_LIMBS],
    pub wrong_modulus_in_native_modulus: N,

    pub max_reduced_limb: big_uint,
    pub max_unreduced_limb: big_uint,
    pub max_remainder: big_uint,
    pub max_operand: big_uint,
    pub max_mul_quotient: big_uint,

    pub max_most_significant_reduced_limb: big_uint,
    pub max_most_significant_operand_limb: big_uint,
    pub max_most_significant_mul_quotient_limb: big_uint,

    pub mul_v_bit_len: usize,
    pub red_v_bit_len: usize,

    _marker_wrong: PhantomData<W>,
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    // Calculates base auxillary value for subtraction such that auxillary value
    // must be equal to `wrong_modulus` and all limbs of it must be higher than
    // dense limb value.
    fn calculate_base_aux() -> [big_uint; NUMBER_OF_LIMBS] {
        let two = N::from(2);
        let r = &fe_to_big(two.pow(&[BIT_LEN_LIMB as u64, 0, 0, 0]));
        let wrong_modulus = modulus::<W>();
        let wrong_modulus: Vec<N> = decompose_big(wrong_modulus, NUMBER_OF_LIMBS, BIT_LEN_LIMB);

        // `base_aux = 2 * wrong_modulus`
        let mut base_aux: Vec<big_uint> = wrong_modulus
            .into_iter()
            .map(|limb| fe_to_big(limb) << 1usize)
            .collect();

        // If value of a limb is not above dense limb borrow from the next one
        for i in 0..NUMBER_OF_LIMBS - 1 {
            let hidx = NUMBER_OF_LIMBS - i - 1;
            let lidx = hidx - 1;

            if (base_aux[lidx].bits() as usize) < (BIT_LEN_LIMB + 1) {
                base_aux[hidx] = base_aux[hidx].clone() - 1usize;
                base_aux[lidx] = base_aux[lidx].clone() + r;
            }
        }

        base_aux.try_into().unwrap()
    }

    pub fn construct() -> Self {
        assert!(NUMBER_OF_LIMBS > 2);

        // Limitation of range chip!
        assert!(BIT_LEN_LIMB % 4 == 0);

        let one = &big_uint::one();

        // previous power of two
        macro_rules! log_floor {
            ($u:expr) => {
                &(one << ($u.bits() as usize - 1))
            };
        }

        // next power of two
        macro_rules! log_ceil {
            ($u:expr) => {
                &(one << $u.bits() as usize)
            };
        }

        // `t = BIT_LEN_LIMB * NUMBER_OF_LIMBS`
        // `T = 2 ^ t` which we also name as `binary_modulus`
        let binary_modulus_bit_len = BIT_LEN_LIMB * NUMBER_OF_LIMBS;
        let binary_modulus = &(one << binary_modulus_bit_len);

        // wrong field modulus: `w`
        let wrong_modulus = &modulus::<W>();
        // native field modulus: `n`
        let native_modulus = &modulus::<N>();

        // Multiplication is constrained as:
        //
        // `a * b = w * quotient + remainder`
        //
        // where `quotient` and `remainder` is witnesses, `a` and `b` are assigned
        // operands. Both sides of the equation must not wrap `crt_modulus`.
        let crt_modulus = &(binary_modulus * native_modulus);

        // Witness remainder might overflow the wrong modulus but it is limited
        // to the next power of two of the wrong modulus.
        let max_remainder = &(log_ceil!(wrong_modulus) - one);

        // Find maxium quotient that won't wrap `quotient * wrong + remainder` side of
        // the equation under `crt_modulus`.
        let pre_max_quotient = &((crt_modulus - max_remainder) / wrong_modulus);
        // Lower this value to make this value suitable for bit range checks.
        let max_quotient = &(log_floor!(pre_max_quotient) - one);

        // Find the maximum operand: in order to meet completeness maximum allowed
        // operand value is saturated as below:
        //
        // `max_operand ^ 2 < max_quotient * wrong + max_remainder`
        //
        // So that prover can find `quotient` and `remainder` witnesses for any
        // allowed input operands. And it also automativally ensures that:
        //
        // `max_operand^2 < crt_modulus`
        //
        // must hold.
        let max_operand_bit_len = ((max_quotient * wrong_modulus + max_remainder).bits() - 1) / 2;
        let max_operand = &((one << max_operand_bit_len) - one);

        // Sanity check
        {
            let lhs = &(max_operand * max_operand);
            let rhs = &(max_quotient * wrong_modulus + max_remainder);

            assert!(binary_modulus > wrong_modulus);
            assert!(binary_modulus > native_modulus);

            assert!(max_remainder > wrong_modulus);
            assert!(max_operand > wrong_modulus);
            assert!(max_quotient > wrong_modulus);

            assert!(max_remainder < binary_modulus);
            assert!(max_operand < binary_modulus);
            assert!(max_quotient < binary_modulus);

            assert!(rhs < crt_modulus);
            assert!(lhs < rhs);
        }

        // negative wrong field modulus moduli binary modulus `w'`
        // `w' = (T - w)`
        // `w' = [w'_0, w'_1, ... ]`
        let negative_wrong_modulus_decomposed: [N; NUMBER_OF_LIMBS] = decompose_big(
            binary_modulus - wrong_modulus.clone(),
            NUMBER_OF_LIMBS,
            BIT_LEN_LIMB,
        )
        .try_into()
        .unwrap();

        // `w = [w_0, w_1, ... ]`
        let wrong_modulus_decomposed =
            decompose_big(wrong_modulus.clone(), NUMBER_OF_LIMBS, BIT_LEN_LIMB)
                .try_into()
                .unwrap();

        // `w-1 = [w_0-1 , w_1, ... ] `
        let wrong_modulus_minus_one = decompose_big(
            wrong_modulus.clone() - 1usize,
            NUMBER_OF_LIMBS,
            BIT_LEN_LIMB,
        )
        .try_into()
        .unwrap();

        // Full dense limb without overflow
        let max_reduced_limb = &(one << BIT_LEN_LIMB) - one;

        // Keep this much lower than what we can reduce with single limb quotient to
        // take extra measure for overflow issues
        let max_unreduced_limb = &(one << (BIT_LEN_LIMB + BIT_LEN_LIMB / 2)) - one;

        // Most significant limbs are subjected to different range checks which will be
        // probably less than full sized limbs.
        let max_most_significant_reduced_limb =
            &(max_remainder >> ((NUMBER_OF_LIMBS - 1) * BIT_LEN_LIMB));
        let max_most_significant_operand_limb =
            &(max_operand >> ((NUMBER_OF_LIMBS - 1) * BIT_LEN_LIMB));
        let max_most_significant_mul_quotient_limb =
            &(max_quotient >> ((NUMBER_OF_LIMBS - 1) * BIT_LEN_LIMB));

        // Emulate a multiplication to find out max residue overflows:
        let mut mul_v_bit_len: usize = BIT_LEN_LIMB;
        {
            // Maximum operand
            let a = (0..NUMBER_OF_LIMBS)
                .map(|i| {
                    if i != NUMBER_OF_LIMBS - 1 {
                        max_reduced_limb.clone()
                    } else {
                        max_most_significant_operand_limb.clone()
                    }
                })
                .collect::<Vec<big_uint>>();

            let p: Vec<big_uint> = negative_wrong_modulus_decomposed
                .iter()
                .map(|e| fe_to_big(*e))
                .collect();

            // Maximum quotient
            let q = (0..NUMBER_OF_LIMBS)
                .map(|i| {
                    if i != NUMBER_OF_LIMBS - 1 {
                        max_reduced_limb.clone()
                    } else {
                        max_most_significant_mul_quotient_limb.clone()
                    }
                })
                .collect::<Vec<big_uint>>();

            // Find intermediate maximums
            let mut t = vec![big_uint::zero(); 2 * NUMBER_OF_LIMBS - 1];
            for i in 0..NUMBER_OF_LIMBS {
                for j in 0..NUMBER_OF_LIMBS {
                    t[i + j] = &t[i + j] + &a[i] * &a[j] + &p[i] * &q[j];
                }
            }

            let is_odd = NUMBER_OF_LIMBS % 1 == 1;
            let u_len = (NUMBER_OF_LIMBS + 1) / 2;

            let mut carry = big_uint::zero();
            for i in 0..u_len {
                let v = if (i == u_len - 1) && is_odd {
                    // odd and last iter
                    let u = &t[i] + &carry;
                    u >> BIT_LEN_LIMB
                } else {
                    let u = &t[i] + (&t[i + 1] << BIT_LEN_LIMB) + &carry;
                    u >> (2 * BIT_LEN_LIMB)
                };
                carry = v.clone();
                mul_v_bit_len = std::cmp::max(v.bits() as usize, mul_v_bit_len)
            }
        };

        // Emulate a multiplication to find out max residue overflows:
        let mut red_v_bit_len: usize = BIT_LEN_LIMB;
        {
            // Maximum operand
            let a = (0..NUMBER_OF_LIMBS)
                .map(|i| {
                    if i != NUMBER_OF_LIMBS - 1 {
                        max_reduced_limb.clone()
                    } else {
                        max_most_significant_operand_limb.clone()
                    }
                })
                .collect::<Vec<big_uint>>();

            let p: Vec<big_uint> = negative_wrong_modulus_decomposed
                .iter()
                .map(|e| fe_to_big(*e))
                .collect();

            // Maximum quorient
            let q = (0..NUMBER_OF_LIMBS)
                .map(|i| {
                    if i != NUMBER_OF_LIMBS - 1 {
                        max_reduced_limb.clone()
                    } else {
                        max_most_significant_mul_quotient_limb.clone()
                    }
                })
                .collect::<Vec<big_uint>>();

            // Find intermediate maximums
            let mut t = vec![big_uint::zero(); 2 * NUMBER_OF_LIMBS - 1];
            for i in 0..NUMBER_OF_LIMBS {
                for j in 0..NUMBER_OF_LIMBS {
                    t[i + j] = &t[i + j] + &a[i] + &p[i] * &q[j];
                }
            }

            let is_odd = NUMBER_OF_LIMBS & 1 == 1;
            let u_len = (NUMBER_OF_LIMBS + 1) / 2;

            let mut carry = big_uint::zero();
            for i in 0..u_len {
                let v = if (i == u_len - 1) && is_odd {
                    // odd and last iter
                    let u = &t[i] + &carry;
                    u >> BIT_LEN_LIMB
                } else {
                    let u = &t[i] + (&t[i + 1] << BIT_LEN_LIMB) + &carry;
                    u >> (2 * BIT_LEN_LIMB)
                };
                carry = v.clone();
                red_v_bit_len = std::cmp::max(v.bits() as usize, red_v_bit_len)
            }
        };

        let bit_len_lookup = BIT_LEN_LIMB / NUMBER_OF_LOOKUP_LIMBS;
        // Assert that bit length of limbs is divisible by sub limbs for lookup
        assert!(bit_len_lookup * NUMBER_OF_LOOKUP_LIMBS == BIT_LEN_LIMB);

        // Calculate auxillary value for subtraction
        let base_aux = Self::calculate_base_aux();
        // Sanity check for auxillary value
        {
            let base_aux_value = compose(base_aux.to_vec(), BIT_LEN_LIMB);
            // Must be equal to wrong modulus
            assert!(base_aux_value.clone() % wrong_modulus == big_uint::zero());
            // Expected to be above next power of two
            assert!(base_aux_value > *max_remainder);

            // Assert limbs are above max values
            for (i, aux) in base_aux.iter().enumerate() {
                let is_last_limb = i == NUMBER_OF_LIMBS - 1;
                let target = if is_last_limb {
                    max_most_significant_reduced_limb.clone()
                } else {
                    max_reduced_limb.clone()
                };
                assert!(*aux >= target);
            }
        }

        let wrong_modulus_in_native_modulus: N =
            big_to_fe(wrong_modulus.clone() % native_modulus.clone());

        // Calculate shifter elements
        let two = N::from(2);
        let two_inv = two.invert().unwrap();

        // Right shifts field element by `u * BIT_LEN_LIMB` bits
        let right_shifters = (0..NUMBER_OF_LIMBS)
            .map(|i| two_inv.pow(&[(i * BIT_LEN_LIMB) as u64, 0, 0, 0]))
            .collect::<Vec<N>>()
            .try_into()
            .unwrap();

        // Left shifts field element by `u * BIT_LEN_LIMB` bits
        let left_shifters = (0..NUMBER_OF_LIMBS)
            .map(|i| two.pow(&[(i * BIT_LEN_LIMB) as u64, 0, 0, 0]))
            .collect::<Vec<N>>()
            .try_into()
            .unwrap();

        let rns = Rns {
            bit_len_lookup,

            right_shifters,
            left_shifters,

            wrong_modulus: wrong_modulus.clone(),
            native_modulus: native_modulus.clone(),
            binary_modulus: binary_modulus.clone(),
            crt_modulus: crt_modulus.clone(),

            base_aux,

            negative_wrong_modulus_decomposed,
            wrong_modulus_decomposed,
            wrong_modulus_minus_one,
            wrong_modulus_in_native_modulus,

            max_reduced_limb: max_reduced_limb.clone(),
            max_unreduced_limb: max_unreduced_limb.clone(),
            max_remainder: max_remainder.clone(),
            max_operand: max_operand.clone(),
            max_mul_quotient: max_quotient.clone(),

            max_most_significant_reduced_limb: max_most_significant_reduced_limb.clone(),
            max_most_significant_operand_limb: max_most_significant_operand_limb.clone(),
            max_most_significant_mul_quotient_limb: max_most_significant_mul_quotient_limb.clone(),

            mul_v_bit_len,
            red_v_bit_len,

            _marker_wrong: PhantomData,
        };

        // Another sanity check for maximum reducible value:
        {
            let max_with_max_unreduced_limbs = &[big_to_fe(max_unreduced_limb); NUMBER_OF_LIMBS];
            let max_with_max_unreduced =
                Integer::from_limbs(max_with_max_unreduced_limbs, Rc::new(rns.clone()));
            let reduction_result = max_with_max_unreduced.reduce();
            let quotient = match reduction_result.quotient {
                Quotient::Short(quotient) => quotient,
                _ => panic!("short quotient is expected"),
            };
            let quotient = fe_to_big(quotient);
            assert!(quotient < max_reduced_limb);
        }

        rns
    }

    pub fn right_shifter(&self, i: usize) -> N {
        self.right_shifters[i]
    }

    pub fn left_shifter(&self, i: usize) -> N {
        self.left_shifters[i]
    }

    pub fn overflow_lengths(&self) -> Vec<usize> {
        let max_most_significant_mul_quotient_limb_size =
            self.max_most_significant_mul_quotient_limb.bits() as usize % self.bit_len_lookup;
        let max_most_significant_operand_limb_size =
            self.max_most_significant_operand_limb.bits() as usize % self.bit_len_lookup;
        let max_most_significant_reduced_limb_size =
            self.max_most_significant_reduced_limb.bits() as usize % self.bit_len_lookup;
        vec![
            self.mul_v_bit_len % self.bit_len_lookup,
            self.red_v_bit_len % self.bit_len_lookup,
            max_most_significant_mul_quotient_limb_size,
            max_most_significant_operand_limb_size,
            max_most_significant_reduced_limb_size,
        ]
    }
}

#[derive(Debug, Clone)]
pub struct Limb<F: FieldExt>(F);

impl<F: FieldExt> Common<F> for Limb<F> {
    fn value(&self) -> big_uint {
        fe_to_big(self.0)
    }
}

impl<F: FieldExt> Default for Limb<F> {
    fn default() -> Self {
        Limb(F::zero())
    }
}

impl<F: FieldExt> From<big_uint> for Limb<F> {
    fn from(e: big_uint) -> Self {
        Self(big_to_fe(e))
    }
}

impl<F: FieldExt> From<&str> for Limb<F> {
    fn from(e: &str) -> Self {
        Self(big_to_fe(big_uint::from_str_radix(e, 16).unwrap()))
    }
}

impl<F: FieldExt> Limb<F> {
    pub(crate) fn new(value: F) -> Self {
        Limb(value)
    }

    pub(crate) fn from_big(e: big_uint) -> Self {
        Self::new(big_to_fe(e))
    }

    pub(crate) fn fe(&self) -> F {
        self.0
    }
}

// Integer is a decomposed scalar field value with limbs that are in native
// field.
#[derive(Clone)]
pub struct Integer<
    W: WrongExt,
    N: FieldExt,
    const NUMBER_OF_LIMBS: usize,
    const BIT_LEN_LIMB: usize,
> {
    limbs: Vec<Limb<N>>,
    rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>,
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize> fmt::Debug
    for Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let value = self.value();
        let mut debug = f.debug_struct("Integer");
        debug.field("value", &value.to_str_radix(16));
        for (i, limb) in self.limbs().iter().enumerate() {
            let value = fe_to_big(*limb);
            debug.field(&format!("limb {}", i), &value.to_str_radix(16));
        }
        debug.finish()?;
        Ok(())
    }
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize> Common<N>
    for Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    fn value(&self) -> big_uint {
        let limb_values = self.limbs.iter().map(|limb| limb.value()).collect();
        compose(limb_values, BIT_LEN_LIMB)
    }
}

impl<W: WrongExt, N: FieldExt, const NUMBER_OF_LIMBS: usize, const BIT_LEN_LIMB: usize>
    Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>
{
    pub fn new(limbs: Vec<Limb<N>>, rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        assert!(limbs.len() == NUMBER_OF_LIMBS);
        Self { limbs, rns }
    }

    pub fn from_fe(e: W, rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        Integer::from_big(fe_to_big(e), rns)
    }

    pub fn from_big(e: big_uint, rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        let limbs = decompose_big::<N>(e, NUMBER_OF_LIMBS, BIT_LEN_LIMB);
        let limbs = limbs.iter().map(|e| Limb::<N>::new(*e)).collect();
        Self { limbs, rns }
    }

    pub fn from_limbs(
        limbs: &[N; NUMBER_OF_LIMBS],
        rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>,
    ) -> Self {
        let limbs = limbs.iter().map(|limb| Limb::<N>::new(*limb)).collect();
        Integer { limbs, rns }
    }

    pub fn from_bytes_le(e: &[u8], rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>) -> Self {
        let x = num_bigint::BigUint::from_bytes_le(e);
        Self::from_big(x, rns)
    }

    pub fn limbs(&self) -> Vec<N> {
        self.limbs.iter().map(|limb| limb.fe()).collect()
    }

    pub fn limb(&self, idx: usize) -> Limb<N> {
        self.limbs[idx].clone()
    }

    pub fn scale(&mut self, k: N) {
        for limb in self.limbs.iter_mut() {
            limb.0 *= k;
        }
    }

    // Returns inversion witnesses
    pub(crate) fn invert(&self) -> Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>> {
        let a_biguint = self.value();
        let a_w = big_to_fe::<W>(a_biguint);
        let inv_w = a_w.invert();
        inv_w
            .map(|inv| Self::from_big(fe_to_big(inv), Rc::clone(&self.rns)))
            .into()
    }

    // Returns division witnesses
    pub(crate) fn div(
        &self,
        denom: &Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>,
    ) -> Option<Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>> {
        denom.invert().map(|b_inv| {
            let a_mul_b = (self.value() * b_inv.value()) % self.rns.wrong_modulus.clone();
            Self::from_big(a_mul_b, Rc::clone(&self.rns))
        })
    }

    pub(crate) fn square(&self) -> ReductionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB> {
        self.mul(self)
    }

    // Returns multiplication witnesses
    pub(crate) fn mul(
        &self,
        other: &Integer<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>,
    ) -> ReductionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB> {
        let modulus = self.rns.wrong_modulus.clone();
        let negative_modulus = self.rns.negative_wrong_modulus_decomposed;
        let (quotient, result) = (self.value() * other.value()).div_rem(&modulus);
        let quotient = Self::from_big(quotient, Rc::clone(&self.rns));
        let result = Self::from_big(result, Rc::clone(&self.rns));

        let l = NUMBER_OF_LIMBS;
        let mut t: Vec<N> = vec![N::zero(); l];
        for k in 0..l {
            for i in 0..=k {
                let j = k - i;
                t[i + j] = t[i + j]
                    + self.limb(i).0 * other.limb(j).0
                    + negative_modulus[i] * quotient.limb(j).0;
            }
        }

        let t = t.try_into().unwrap();
        let residues = result.residues(&t);

        ReductionWitness {
            result,
            intermediate: t,
            quotient: Quotient::Long(quotient),
            residues,
        }
    }

    // Returns reduction witnesses
    pub(crate) fn reduce(&self) -> ReductionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB> {
        let modulus = self.rns.wrong_modulus.clone();
        let negative_modulus = self.rns.negative_wrong_modulus_decomposed;

        let (quotient, result) = self.value().div_rem(&modulus);
        assert!(quotient < big_uint::one() << BIT_LEN_LIMB);

        let quotient: N = big_to_fe(quotient);
        let t: [N; NUMBER_OF_LIMBS] = self
            .limbs()
            .iter()
            .zip(negative_modulus.iter())
            .map(|(a, p)| *a + *p * quotient)
            .collect::<Vec<N>>()
            .try_into()
            .unwrap();

        let result = Integer::from_big(result, Rc::clone(&self.rns));
        let residues = result.residues(&t);

        ReductionWitness {
            result,
            intermediate: t,
            quotient: Quotient::Short(quotient),
            residues,
        }
    }

    fn residues(&self, t: &[N; NUMBER_OF_LIMBS]) -> Vec<N> {
        let is_odd = NUMBER_OF_LIMBS & 1 == 1;
        let u_len = (NUMBER_OF_LIMBS + 1) / 2;
        let lsh1 = self.rns.left_shifter(1);
        let (rsh1, rsh2) = (self.rns.right_shifter(1), self.rns.right_shifter(2));

        let mut carry = N::zero();
        // TODO: use chunks
        (0..u_len)
            .map(|i| {
                let j = 2 * i;
                let v = if (i == u_len - 1) && is_odd {
                    let r = self.limb(j).0;
                    let u = t[j] - r;
                    u * rsh1
                } else {
                    let (r_0, r_1) = (self.limb(j).0, self.limb(j + 1).0);
                    let (t_0, t_1) = (t[j], t[j + 1]);
                    let u = t_0 + (t_1 * lsh1) - r_0 - (lsh1 * r_1) + carry;
                    u * rsh2
                };
                carry = v;
                v
            })
            .collect()
    }

    // Returns comparision witnesses
    pub(crate) fn compare_to_modulus(
        &self,
    ) -> ComparisionWitness<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB> {
        let mut borrow = [false; NUMBER_OF_LIMBS];
        let modulus_minus_one = self.rns.wrong_modulus_minus_one;

        let mut prev_borrow = big_uint::zero();
        let limbs = self
            .limbs
            .iter()
            .zip(modulus_minus_one.iter())
            .zip(borrow.iter_mut())
            .map(|((limb, modulus_limb), borrow)| {
                let limb = &limb.value();
                let modulus_limb = fe_to_big(*modulus_limb);
                let cur_borrow = modulus_limb < limb + prev_borrow.clone();
                *borrow = cur_borrow;
                let cur_borrow = bool_to_big(cur_borrow) << BIT_LEN_LIMB;
                let res_limb = ((modulus_limb + cur_borrow) - prev_borrow.clone()) - limb;
                prev_borrow = bool_to_big(*borrow);

                big_to_fe(res_limb)
            })
            .collect::<Vec<N>>()
            .try_into()
            .unwrap();

        let result = Integer::from_limbs(&limbs, Rc::clone(&self.rns));
        ComparisionWitness { result, borrow }
    }

    // Construct a new integer that equals to the modulus and its max limb values
    // are higher than the given max values
    pub fn subtracion_aux(
        max_vals: &[big_uint; NUMBER_OF_LIMBS],
        rns: Rc<Rns<W, N, NUMBER_OF_LIMBS, BIT_LEN_LIMB>>,
    ) -> Self {
        let mut max_shift = 0usize;
        for (max_val, aux) in max_vals.iter().zip(rns.base_aux.iter()) {
            let mut shift = 1;
            let mut aux = aux.clone();
            while *max_val > aux {
                aux <<= 1usize;
                max_shift = std::cmp::max(shift, max_shift);
                shift += 1;
            }
        }
        let limbs = rns
            .base_aux
            .iter()
            .map(|aux_limb| big_to_fe(aux_limb << max_shift))
            .collect::<Vec<N>>()
            .try_into()
            .unwrap();
        Self::from_limbs(&limbs, rns)
    }
}
