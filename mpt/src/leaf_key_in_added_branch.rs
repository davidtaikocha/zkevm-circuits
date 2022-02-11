use halo2::{
    circuit::Chip,
    plonk::{
        Advice, Column, ConstraintSystem, Expression, Fixed, VirtualCells,
    },
    poly::Rotation,
};
use pairing::arithmetic::FieldExt;
use std::marker::PhantomData;

use crate::mpt::FixedTableTag;

use crate::param::{
    HASH_WIDTH, IS_BRANCH_C_PLACEHOLDER_POS, IS_BRANCH_S_PLACEHOLDER_POS,
    KECCAK_INPUT_WIDTH, KECCAK_OUTPUT_WIDTH, LAYOUT_OFFSET, R_TABLE_LEN,
};

#[derive(Clone, Debug)]
pub(crate) struct LeafKeyInAddedBranchConfig {}

pub(crate) struct LeafKeyInAddedBranchChip<F> {
    config: LeafKeyInAddedBranchConfig,
    _marker: PhantomData<F>,
}

impl<F: FieldExt> LeafKeyInAddedBranchChip<F> {
    pub fn configure(
        meta: &mut ConstraintSystem<F>,
        q_enable: impl Fn(&mut VirtualCells<'_, F>) -> Expression<F>,
        s_rlp1: Column<Advice>,
        s_rlp2: Column<Advice>,
        c_rlp1: Column<Advice>,
        s_advices: [Column<Advice>; HASH_WIDTH],
        s_keccak: [Column<Advice>; KECCAK_OUTPUT_WIDTH], // to check hash && to see whether it's long or short RLP
        c_keccak: [Column<Advice>; KECCAK_OUTPUT_WIDTH], // to check hash && to see whether it's long or short RLP
        acc: Column<Advice>,
        acc_mult: Column<Advice>,
        sel1: Column<Advice>,
        sel2: Column<Advice>,
        key_rlc: Column<Advice>,
        key_rlc_mult: Column<Advice>,
        mult_diff: Column<Advice>,
        drifted_pos: Column<Advice>,
        r_table: Vec<Expression<F>>,
        fixed_table: [Column<Fixed>; 3],
        keccak_table: [Column<Fixed>; KECCAK_INPUT_WIDTH + KECCAK_OUTPUT_WIDTH],
    ) -> LeafKeyInAddedBranchConfig {
        let config = LeafKeyInAddedBranchConfig {};

        // TODO: after key_len there are 0s

        let one = Expression::Constant(F::from(1_u64));
        let c16 = Expression::Constant(F::from(16_u64));
        let c32 = Expression::Constant(F::from(32_u64));
        let c48 = Expression::Constant(F::from(48_u64));
        let c248 = Expression::Constant(F::from(248_u64));
        let rot_branch_init = -23;

        // Checking leaf RLC is ok - RLC is then taken and value (from leaf_value row) is added
        // to RLC, finally lookup is used to check the hash that
        // corresponds to this RLC is in the parent branch at drifted_pos position.
        // This is not to be confused with key RLC checked in another gate (the gate
        // here checks the RLC of all leaf bytes, while the gate below checks the key RLC
        // accumulated in branches/extensions + leaf key).
        meta.create_gate("Storage leaf in added branch RLC", |meta| {
            let q_enable = q_enable(meta);
            let mut constraints = vec![];

            // TODO: is_long, is_short are booleans
            // TODO: is_long + is_short = 1

            // TODO: check there is 248 in long

            let s_rlp1 = meta.query_advice(s_rlp1, Rotation::cur());

            // TODO: check that from some point on (depends on the rlp meta data)
            // the values are zero (as in key_compr) - but take into account it can be long or short RLP

            let mut rlc = s_rlp1;
            let s_rlp2 = meta.query_advice(s_rlp2, Rotation::cur());
            rlc = rlc + s_rlp2 * r_table[0].clone();
            let mut rind = 1;

            let mut r_wrapped = false;
            for col in s_advices.iter() {
                let s = meta.query_advice(*col, Rotation::cur());
                if !r_wrapped {
                    rlc = rlc + s * r_table[rind].clone();
                } else {
                    rlc = rlc
                        + s * r_table[rind].clone()
                            * r_table[R_TABLE_LEN - 1].clone();
                }
                if rind == R_TABLE_LEN - 1 {
                    rind = 0;
                    r_wrapped = true;
                } else {
                    rind += 1;
                }
            }

            let c_rlp1 = meta.query_advice(c_rlp1, Rotation::cur());
            rlc = rlc
                + c_rlp1
                    * r_table[R_TABLE_LEN - 1].clone()
                    * r_table[1].clone();

            // key is at most of length 32, so it doesn't go further than c_rlp1

            let acc = meta.query_advice(acc, Rotation::cur());
            constraints.push(("Leaf key acc", q_enable * (rlc - acc)));

            constraints
        });

        /*
        // We also need to check leaf_key and leaf_key_in_added_branch are different only
        // in the drifted_pos. This ensures the leaf
        // that was turned into branch was moved down to the new branch correctly.
        meta.create_gate(
            "Storage leaf in added branch differs only in first nibble (sel2, is_short)",
            |meta| {
                let q_enable = q_enable(meta);
                let mut constraints = vec![];

                let is_branch_s_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_S_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_branch_c_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_C_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_short = meta.query_advice(s_keccak[1], Rotation::cur());

                // If sel2 = 1 and is_short, the leaf_key has the first nibble
                // in s_advices[0].
                // Note that due to placeholder branch, sel1 and sel2 are turned around.

                // [226, 160, 32 + 16 + 7, 5 * 16 + 8, 9 * 16 + 12,
                // [226, 160, 32,          5 * 16 + 8, 9 * 16 + 12

                // The first nibble is removed in leaf_key_in_added_branch.
                // So, s_rlp1 is the same in both rows.
                // Also s_rlp2 is the same in both rows.
                // Further, s_advices[0]_leaf_key_in_added_branch = 32 and
                // s_advices[0]_leaf_key = 32 + 16 + first_nibble.
                // From s_advices[0] on, key bytes are the same in both rows.

                let rot_leaf_key_s = -4;
                let rot_leaf_key_c = -2;

                // sel1 and sel2 are in init branch
                let sel2 = meta.query_advice(sel2, Rotation(rot_branch_init));

                let s_rlp1_prev_s = meta.query_advice(s_rlp1, Rotation(rot_leaf_key_s));
                let s_rlp1_prev_c = meta.query_advice(s_rlp1, Rotation(rot_leaf_key_c));
                let s_rlp1 = meta.query_advice(s_rlp1, Rotation::cur());
                let s_rlp2_prev_s = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_s));
                let s_rlp2_prev_c = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_c));
                let s_rlp2 = meta.query_advice(s_rlp2, Rotation::cur());

                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_rlp1.clone() - s_rlp1_prev_s),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_rlp2.clone() - s_rlp2_prev_s),
                ));

                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_rlp1 - s_rlp1_prev_c),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_rlp2 - s_rlp2_prev_c),
                ));

                let s_advices0_prev_s = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_s));
                let s_advices0_prev_c = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_c));
                let s_advices0 = meta.query_advice(s_advices[0], Rotation::cur());

                // Any rotation that lands into branch children can be used.
                let first_nibble = meta.query_advice(drifted_pos, Rotation(-17));

                constraints.push((
                    "Leaf key differs first nibble s_advices[0] prev placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_advices0.clone() - c32.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] prev placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_advices0 - c32.clone()),
                ));

                constraints.push((
                    "Leaf key differs first nibble s_advices[0] placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_advices0_prev_s - first_nibble.clone() - c48.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_short.clone()
                        * (s_advices0_prev_c - first_nibble - c48.clone()),
                ));

                for col in s_advices.iter().skip(1) {
                    let s_prev_s = meta.query_advice(*col, Rotation(rot_leaf_key_s));
                    let s_prev_c = meta.query_advice(*col, Rotation(rot_leaf_key_c));
                    let s = meta.query_advice(*col, Rotation::cur());

                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder s",
                            q_enable.clone()
                            * is_branch_s_placeholder.clone()
                            * sel2.clone()
                            * is_short.clone()
                            * (s_prev_s - s.clone()),
                    ));
                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder c",
                            q_enable.clone()
                            * is_branch_c_placeholder.clone()
                            * sel2.clone()
                            * is_short.clone()
                            * (s_prev_c - s),
                    ));
                }

                // key is at most of length 32 and this is short RLP,
                // so key doesn't go further than s_advices

                constraints
            },
        );

        meta.create_gate(
            "Storage leaf in added branch differs only in first nibble (sel1, is_short)",
            |meta| {
                let q_enable = q_enable(meta);
                let mut constraints = vec![];

                let is_branch_s_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_S_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_branch_c_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_C_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_short = meta.query_advice(s_keccak[1], Rotation::cur());

                // If sel1 = 1 and is_short, the leaf_key has 32 in s_advices[0].
                // Note that due to placeholder branch, sel1 and sel2 are turned around.

                // [226, 160, 32,          7 * 16 + 5, 8 * 16 + 9,
                // [225, 159, 32 + 16 + 5, 8 * 16 + 9,

                // The first nibble (7 in the example) is in s_advices[1],
                // this nibble is removed in leaf_key_in_added_branch.
                // The second nibble in s_advices[1] (5 in the example) moves
                // in leaf_key_in_added_branch into s_advices[0].

                // So, s_rlp1 differs by 1.
                // Also s_rlp2 is smaller for 1 in added branch.
                // Further,
                // s_advices[0]_leaf_key_in_added_branch = 32 + 16 + second_nibble
                // where second_nibble = s_advices[1]_leaf_key - first_nibble * 16

                // From s_advices[1] on, key bytes are the same, but shifted for one position.

                let rot_branch_init = -23;
                let rot_leaf_key_s = -4;
                let rot_leaf_key_c = -2;

                // sel1 and sel2 are in init branch
                let sel1 = meta.query_advice(sel1, Rotation(rot_branch_init));

                let s_rlp1_prev_s = meta.query_advice(s_rlp1, Rotation(rot_leaf_key_s));
                let s_rlp1_prev_c = meta.query_advice(s_rlp1, Rotation(rot_leaf_key_c));
                let s_rlp1 = meta.query_advice(s_rlp1, Rotation::cur());
                let s_rlp2_prev_s = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_s));
                let s_rlp2_prev_c = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_c));
                let s_rlp2 = meta.query_advice(s_rlp2, Rotation::cur());

                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_rlp1.clone() - s_rlp1_prev_s + one.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_rlp2.clone() - s_rlp2_prev_s + one.clone()),
                ));

                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_rlp1 - s_rlp1_prev_c + one.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_rlp2 - s_rlp2_prev_c + one.clone()),
                ));

                let s_advices0_prev_s = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_s));
                let s_advices0_prev_c = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_c));
                let s_advices1_prev_s = meta.query_advice(s_advices[1], Rotation(rot_leaf_key_s));
                let s_advices1_prev_c = meta.query_advice(s_advices[1], Rotation(rot_leaf_key_c));
                let s_advices0 = meta.query_advice(s_advices[0], Rotation::cur());

                // Any rotation that lands into branch children can be used.
                let first_nibble = meta.query_advice(drifted_pos, Rotation(-17));
                let second_nibble_s = s_advices1_prev_s - first_nibble.clone() * c16.clone();
                let second_nibble_c = s_advices1_prev_c - first_nibble * c16.clone();

                constraints.push((
                    "Leaf key differs first nibble s_advices[0] prev placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_advices0.clone() - c48.clone() - second_nibble_s),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] prev placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_advices0 - c48.clone() - second_nibble_c),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_advices0_prev_s - c32.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_short.clone()
                        * (s_advices0_prev_c - c32.clone()),
                ));

                for ind in 2..HASH_WIDTH {
                    let s_prev_s = meta.query_advice(s_advices[ind], Rotation(rot_leaf_key_s));
                    let s_prev_c = meta.query_advice(s_advices[ind], Rotation(rot_leaf_key_c));
                    let s = meta.query_advice(s_advices[ind-1], Rotation::cur());

                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder s",
                            q_enable.clone()
                            * is_branch_s_placeholder.clone()
                            * sel1.clone()
                            * is_short.clone()
                            * (s_prev_s - s.clone()),
                    ));
                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder c",
                            q_enable.clone()
                            * is_branch_c_placeholder.clone()
                            * sel1.clone()
                            * is_short.clone()
                            * (s_prev_c - s),
                    ));
                }

                // key is at most of length 32 and this is short RLP,
                // so key doesn't go further than s_advices

                constraints
            },
        );

        meta.create_gate(
            "Storage leaf in added branch differs only in first nibble (sel2, is_long)",
            |meta| {
                let q_enable = q_enable(meta);
                let mut constraints = vec![];

                let is_branch_s_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_S_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_branch_c_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_C_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_long = meta.query_advice(s_keccak[0], Rotation::cur());

                // If sel2 = 1 and is_long, the leaf_key has the first nibble
                // in s_advices[1].
                // Note that due to placeholder branch, sel1 and sel2 are turned around.

                // [248, 67, 160, 32 + 16 + 7, 5 * 16 + 8, 9 * 16 + 12,
                // [248, 67, 160, 32,          5 * 16 + 8, 9 * 16 + 12

                // The first nibble is removed in leaf_key_in_added_branch.
                // So, s_rlp1 is the same in both rows.
                // Also s_rlp2 and s_advices[0] are the same in both rows.
                // Further, s_advices[1]_leaf_key_in_added_branch = 32 and
                // s_advices[1]_leaf_key = 32 + 16 + first_nibble.
                // From s_advices[1] on, key bytes are the same in both rows.

                let rot_branch_init = -23;
                let rot_leaf_key_s = -4;
                let rot_leaf_key_c = -2;

                // sel1 and sel2 are in init branch
                let sel2 = meta.query_advice(sel2, Rotation(rot_branch_init));

                let s_rlp1 = meta.query_advice(s_rlp1, Rotation::cur());
                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_rlp1.clone() - c248.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_rlp1.clone() - c248.clone()),
                ));

                let s_rlp2_prev_s = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_s));
                let s_rlp2_prev_c = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_c));
                let s_rlp2 = meta.query_advice(s_rlp2, Rotation::cur());
                let s_advices0_prev_s = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_s));
                let s_advices0_prev_c = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_c));
                let s_advices0 = meta.query_advice(s_advices[0], Rotation::cur());

                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_rlp2.clone() - s_rlp2_prev_s),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices0 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_advices0.clone() - s_advices0_prev_s),
                ));

                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_rlp2 - s_rlp2_prev_c),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices0 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_advices0 - s_advices0_prev_c),
                ));

                let s_advices1_prev_s = meta.query_advice(s_advices[1], Rotation(rot_leaf_key_s));
                let s_advices1_prev_c = meta.query_advice(s_advices[1], Rotation(rot_leaf_key_c));
                let s_advices1 = meta.query_advice(s_advices[1], Rotation::cur());

                // Any rotation that lands into branch children can be used.
                let first_nibble = meta.query_advice(drifted_pos, Rotation(-17));

                constraints.push((
                    "Leaf key differs first nibble s_advices[1] prev placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_advices1.clone() - c32.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[1] prev placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_advices1 - c32.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[1] placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_advices1_prev_s - first_nibble.clone() - c48.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[1] placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (s_advices1_prev_c - first_nibble - c48.clone()),
                ));

                for col in s_advices.iter().skip(2) {
                    let s_prev_s = meta.query_advice(*col, Rotation(rot_leaf_key_s));
                    let s_prev_c = meta.query_advice(*col, Rotation(rot_leaf_key_c));
                    let s = meta.query_advice(*col, Rotation::cur());

                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder s",
                            q_enable.clone()
                            * is_branch_s_placeholder.clone()
                            * sel2.clone()
                            * is_long.clone()
                            * (s_prev_s - s.clone()),
                    ));
                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder c",
                            q_enable.clone()
                            * is_branch_c_placeholder.clone()
                            * sel2.clone()
                            * is_long.clone()
                            * (s_prev_c - s),
                    ));
                }

                // key is at most of length 32 and this is long RLP,
                // so key can go to c_rlp1

                let c_rlp1_prev_s = meta.query_advice(c_rlp1, Rotation(rot_leaf_key_s));
                let c_rlp1_prev_c = meta.query_advice(c_rlp1, Rotation(rot_leaf_key_c));
                let c_rlp1 = meta.query_advice(c_rlp1, Rotation::cur());
                constraints.push((
                    "Leaf key differs first nibble c_rlp1 placeholder s",
                        q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (c_rlp1_prev_s - c_rlp1.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble c_rlp1 placeholder c",
                        q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel2.clone()
                        * is_long.clone()
                        * (c_rlp1_prev_c - c_rlp1),
                ));

                constraints
            },
        );

        meta.create_gate(
            "Storage leaf in added branch differs only in first nibble (sel1, is_long)",
            |meta| {
                let q_enable = q_enable(meta);
                let mut constraints = vec![];

                let is_branch_s_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_S_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_branch_c_placeholder = meta.query_advice(
                    s_advices[IS_BRANCH_C_PLACEHOLDER_POS - LAYOUT_OFFSET],
                    Rotation(-23),
                );
                let is_long = meta.query_advice(s_keccak[0], Rotation::cur());

                // If sel1 = 1 and is_long, the leaf_key has 32 in s_advices[1].
                // Note that due to placeholder branch, sel1 and sel2 are turned around.

                // [248, 67, 160, 32,          7 * 16 + 5, 8 * 16 + 9,
                // [248, 66, 159, 32 + 16 + 5, 8 * 16 + 9,

                // The first nibble (7 in the example) is in s_advices[2],
                // this nibble is removed in leaf_key_in_added_branch.
                // The second nibble in s_advices[2] (5 in the example) moves
                // in leaf_key_in_added_branch into s_advices[1].

                // So, s_rlp1 is the same in both rows.
                // s_rlp2 is smaller for 1 in added branch.
                // s_advices[0] is smaller for 1 in added branch.
                // Further,
                // s_advices[1]_leaf_key_in_added_branch = 32 + 16 + second_nibble
                // where second_nibble = s_advices[2]_leaf_key - first_nibble * 16

                // From s_advices[2] on, key bytes are the same, but shifted for one position.

                let rot_branch_init = -23;
                let rot_leaf_key_s = -4;
                let rot_leaf_key_c = -2;

                // sel1 and sel2 are in init branch
                let sel1 = meta.query_advice(sel1, Rotation(rot_branch_init));

                let s_rlp2_prev_s = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_s));
                let s_rlp2_prev_c = meta.query_advice(s_rlp2, Rotation(rot_leaf_key_c));
                let s_rlp1 = meta.query_advice(s_rlp1, Rotation::cur());
                let s_rlp2 = meta.query_advice(s_rlp2, Rotation::cur());

                // Note that s_rlp1 (=248) in leaf S and C above needs to be checked in leaf_key.

                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_rlp1.clone() - c248.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_rlp2.clone() - s_rlp2_prev_s + one.clone()),
                ));

                constraints.push((
                    "Leaf key differs first nibble s_rlp1 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_rlp1 - c248.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_rlp2 placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_rlp2 - s_rlp2_prev_c + one.clone()),
                ));

                let s_advices0_prev_s = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_s));
                let s_advices0_prev_c = meta.query_advice(s_advices[0], Rotation(rot_leaf_key_c));
                let s_advices0 = meta.query_advice(s_advices[0], Rotation::cur());
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_advices0.clone() - s_advices0_prev_s + one.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[0] placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_advices0 - s_advices0_prev_c + one),
                ));

                let s_advices1_prev_s = meta.query_advice(s_advices[1], Rotation(rot_leaf_key_s));
                let s_advices1_prev_c = meta.query_advice(s_advices[1], Rotation(rot_leaf_key_c));
                let s_advices2_prev_s = meta.query_advice(s_advices[2], Rotation(rot_leaf_key_s));
                let s_advices2_prev_c = meta.query_advice(s_advices[2], Rotation(rot_leaf_key_c));
                let s_advices1 = meta.query_advice(s_advices[1], Rotation::cur());

                // Any rotation that lands into branch children can be used.
                let first_nibble = meta.query_advice(drifted_pos, Rotation(-17));
                let second_nibble_s = s_advices2_prev_s - first_nibble.clone() * c16.clone();
                let second_nibble_c = s_advices2_prev_c - first_nibble * c16.clone();

                constraints.push((
                    "Leaf key differs first nibble s_advices[1] prev placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_advices1.clone() - c48.clone() - second_nibble_s),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[1] prev placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_advices1 - c48.clone() - second_nibble_c),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[1] placeholder s",
                    q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_advices1_prev_s - c32.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble s_advices[1] placeholder c",
                    q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (s_advices1_prev_c - c32),
                ));

                for ind in 3..HASH_WIDTH {
                    let s_prev_s = meta.query_advice(s_advices[ind], Rotation(rot_leaf_key_s));
                    let s_prev_c = meta.query_advice(s_advices[ind], Rotation(rot_leaf_key_c));
                    let s = meta.query_advice(s_advices[ind-1], Rotation::cur());

                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder s",
                            q_enable.clone()
                            * is_branch_s_placeholder.clone()
                            * sel1.clone()
                            * is_long.clone()
                            * (s_prev_s - s.clone()),
                    ));
                    constraints.push((
                        "Leaf key differs first nibble s_advices placeholder c",
                            q_enable.clone()
                            * is_branch_c_placeholder.clone()
                            * sel1.clone()
                            * is_long.clone()
                            * (s_prev_c - s),
                    ));
                }

                // key is at most of length 32 and this is long RLP,
                // so key can go to c_rlp1

                let c_rlp1_prev_s = meta.query_advice(c_rlp1, Rotation(rot_leaf_key_s));
                let c_rlp1_prev_c = meta.query_advice(c_rlp1, Rotation(rot_leaf_key_c));
                let s_advices31 = meta.query_advice(s_advices[HASH_WIDTH-1], Rotation::cur());
                constraints.push((
                    "Leaf key differs first nibble c_rlp1 placeholder s",
                        q_enable.clone()
                        * is_branch_s_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (c_rlp1_prev_s - s_advices31.clone()),
                ));
                constraints.push((
                    "Leaf key differs first nibble c_rlp1 placeholder c",
                        q_enable.clone()
                        * is_branch_c_placeholder.clone()
                        * sel1.clone()
                        * is_long.clone()
                        * (c_rlp1_prev_c - s_advices31),
                ));

                constraints
            },
        );

        */

        // Check acc_mult when RLP metadata is two bytes (short)
        meta.lookup_any(|meta| {
            let q_enable = q_enable(meta);
            let mut constraints = vec![];

            let two = Expression::Constant(F::from(2_u64));

            let is_short = meta.query_advice(s_keccak[1], Rotation::cur());

            let c128 = Expression::Constant(F::from(128));
            let s_rlp2 = meta.query_advice(s_rlp2, Rotation::cur());
            let key_len = s_rlp2 - c128;
            let acc_mult = meta.query_advice(acc_mult, Rotation::cur());

            constraints.push((
                Expression::Constant(F::from(FixedTableTag::RMult as u64)),
                meta.query_fixed(fixed_table[0], Rotation::cur()),
            ));
            constraints.push((
                q_enable.clone() * (key_len + two) * is_short.clone(), // when short, there are 2 RLP meta data
                meta.query_fixed(fixed_table[1], Rotation::cur()),
            ));
            constraints.push((
                q_enable.clone() * acc_mult * is_short,
                meta.query_fixed(fixed_table[2], Rotation::cur()),
            ));

            constraints
        });

        // Check acc_mult when RLP metadata is three bytes (long)
        meta.lookup_any(|meta| {
            let q_enable = q_enable(meta);
            let mut constraints = vec![];

            let three = Expression::Constant(F::from(3_u64));

            let is_long = meta.query_advice(s_keccak[0], Rotation::cur());

            let c128 = Expression::Constant(F::from(128));
            let s_advices0 = meta.query_advice(s_advices[0], Rotation::cur());
            let key_len = s_advices0 - c128;
            let acc_mult = meta.query_advice(acc_mult, Rotation::cur());

            constraints.push((
                Expression::Constant(F::from(FixedTableTag::RMult as u64)),
                meta.query_fixed(fixed_table[0], Rotation::cur()),
            ));
            constraints.push((
                q_enable.clone() * (key_len + three) * is_long.clone(), // when long, there are 3 RLP meta data
                meta.query_fixed(fixed_table[1], Rotation::cur()),
            ));
            constraints.push((
                q_enable.clone() * acc_mult * is_long,
                meta.query_fixed(fixed_table[2], Rotation::cur()),
            ));

            constraints
        });

        // TODO: constraints above could be replaced using key RLC check (checking whether
        // leaf key RLC before extension/branch is added is the same as key RLC of the leaf
        // that drifted into added extension/branch) which would cover
        // also added extension nodes (where we have more than one nibble of difference).
        // It would go like this:
        // We already have leaf key RLC before extension/branch is added. If S is placeholder,
        // we have this RLC in (3) leaf key S row, in key_rlc column.
        // What about key RLC of the drifted leaf? Partial value is already computed in extension
        // node row (key_rlc column) because extension node "takes" the partial key RLC value at
        // leaf (before being drifted) and adds extension key bytes to it. It remains to
        // add drifted_pos and bytes of the drifted leaf. The computations for this are
        // similar to the ones in extension_node_key, the difference is we have here drifted_pos
        // instead of modified_node and we have a different key in a leaf.

        meta.create_gate("work in progress", |meta| {
            let q_enable = q_enable(meta);
            let mut constraints = vec![];

            // Get back into S or C extension row to retrieve key_rlc. Note that this works
            // for both - extension nodes and branches. That's because branch key RLC is stored
            // in extension node row when there is NO extension node (the constraint is in
            // extension_node_key).
            let key_rlc_cur = meta.query_advice(key_rlc, Rotation(-6));

            // Now we have key_rlc after extension node key has been added (in ext_key_rlc),
            // we need to add drifted leaf key now. We need to take into account whether

            // sel1 and sel2 determines whether drifted_pos needs to be
            // multiplied by 16 or not.
            let sel1 = meta.query_advice(sel1, Rotation(rot_branch_init));
            let sel2 = meta.query_advice(sel2, Rotation(rot_branch_init));

            // Note: previous key_rlc in s_keccak[2] and s_keccak[3] could be queried instead.
            let branch_rlc_mult =
                meta.query_advice(key_rlc_mult, Rotation(-30));

            let mult_diff =
                meta.query_advice(mult_diff, Rotation(rot_branch_init + 1));

            let is_long = meta.query_advice(s_keccak[0], Rotation::cur());
            let is_short = meta.query_advice(s_keccak[1], Rotation::cur());

            // Key RLC of the drifted leaf needs to be the same as key RLC of the leaf
            // before it drifted down into extension/branch.
            let is_branch_s_placeholder = meta.query_advice(
                s_advices[IS_BRANCH_S_PLACEHOLDER_POS - LAYOUT_OFFSET],
                Rotation(rot_branch_init),
            );
            let is_branch_c_placeholder = meta.query_advice(
                s_advices[IS_BRANCH_C_PLACEHOLDER_POS - LAYOUT_OFFSET],
                Rotation(rot_branch_init),
            );

            let leaf_key_s_rlc = meta.query_advice(key_rlc, Rotation(-4));
            let leaf_key_c_rlc = meta.query_advice(key_rlc, Rotation(-2));

            // Any rotation that lands into branch children can be used.
            let drifted_pos = meta.query_advice(drifted_pos, Rotation(-17));

            let key_rlc_start = key_rlc_cur.clone()
                + drifted_pos.clone()
                    * c16.clone()
                    * branch_rlc_mult.clone()
                    * mult_diff.clone();

            // If sel1 = 1, we have one nibble+48 in s_advices[0].
            let s_advice0 = meta.query_advice(s_advices[0], Rotation::cur());

            // If sel2 = 1, we have 32 in s_advices[0].
            constraints.push((
                "Leaf key acc s_advice0",
                q_enable.clone()
                    * (s_advice0.clone() - c32.clone())
                    * sel2.clone()
                    * is_short.clone(),
            ));

            let mut key_rlc_short = key_rlc_start.clone()
                + (s_advice0.clone() - c48.clone())
                    * branch_rlc_mult.clone()
                    * sel1.clone()
                    * mult_diff.clone();

            for ind in 1..HASH_WIDTH {
                let s = meta.query_advice(s_advices[ind], Rotation::cur());
                key_rlc_short = key_rlc_short
                    + s * branch_rlc_mult.clone()
                        * mult_diff.clone()
                        * r_table[ind - 1].clone();
            }

            // No need to distinguish between sel1 and sel2 here as it was already
            // when computing key_rlc.
            constraints.push((
                "Drifted leaf key placeholder S",
                q_enable.clone()
                    * is_branch_s_placeholder.clone()
                    * is_short.clone()
                    * (leaf_key_s_rlc.clone() - key_rlc_short.clone()),
            ));
            constraints.push((
                "Drifted leaf key placeholder C",
                q_enable.clone()
                    * is_branch_c_placeholder.clone()
                    * is_short.clone()
                    * (leaf_key_c_rlc.clone() - key_rlc_short.clone()),
            ));

            // Long:
            // Note: long means long leaf RLP, not extension node nibbles.

            // If sel1 = 1, we have one nibble+48 in s_advices[1].
            let s_advice1 = meta.query_advice(s_advices[1], Rotation::cur());

            // If sel2 = 1, we have 32 in s_advices[1].
            constraints.push((
                "Leaf key acc s_advice1",
                q_enable.clone()
                    * (s_advice1.clone() - c32.clone())
                    * sel2.clone()
                    * is_long.clone(),
            ));

            let mut key_rlc_long = key_rlc_start.clone()
                + (s_advice1.clone() - c48.clone())
                    * branch_rlc_mult.clone()
                    * sel1.clone()
                    * mult_diff.clone();

            let mut key_mult = branch_rlc_mult.clone() * mult_diff.clone();
            for ind in 2..HASH_WIDTH {
                let s = meta.query_advice(s_advices[ind], Rotation::cur());
                key_mult = key_mult * r_table[0].clone();
                key_rlc_long = key_rlc_long + s * key_mult.clone();
            }

            key_mult = key_mult * r_table[0].clone();
            let c_rlp1 = meta.query_advice(c_rlp1, Rotation::cur());
            key_rlc_long = key_rlc_long + c_rlp1.clone() * key_mult;

            // No need to distinguish between sel1 and sel2 here as it was already
            // when computing key_rlc.
            constraints.push((
                "Drifted leaf key placeholder S long",
                q_enable.clone()
                    * is_branch_s_placeholder.clone()
                    * is_long.clone()
                    * (leaf_key_s_rlc.clone() - key_rlc_long.clone()),
            ));
            constraints.push((
                "Drifted leaf key placeholder C long",
                q_enable.clone()
                    * is_branch_c_placeholder.clone()
                    * is_long.clone()
                    * (leaf_key_c_rlc.clone() - key_rlc_long.clone()),
            ));

            constraints
        });

        // Checking accumulated RLC for key is not necessary here for leaf_key_in_added_branch
        // because we check this for leaf_key and here we only check the key in leaf_key_in_added_branch
        // corresponds to the one in leaf_key.

        // If the branch is placeholder, we need to check that the leaf without the first
        // nibble has a hash which is in the branch at drifted_pos position.

        // In case we have a placeholder branch at position S:
        // (1) branch (17 rows) which contains leaf that turns into branch at is_modified position (S positions) |
        //     branch (17 rows) that contains added branch hash at is_modified position (C positions)
        // (2) placeholder branch (17 rows) (S positions) | added branch (17 rows) (C positions)
        //     S extension node row
        //     C extension node row
        // (3) leaf key S
        // (4) leaf value S ((3)||(4) hash is two levels above in (1) at is_modified)
        // (5) leaf key C
        // (6) leaf value C ((5)||(6) hash is in one level above (2) at is_modified)
        // (7) leaf in added branch - the same as leaf key S in (3), but it has the first nibble removed

        // We need to check that leaf_in_added_branch hash is in (2) at drifted_pos position
        // (drifted_pos is the first nibble in leaf key S (3), because leaf drifts down to
        // this position in new branch)

        // We need to construct RLP of the leaf. We have leaf key in is_leaf_in_added_branch
        // and the value is the same as it is in the leaf value S (3).

        meta.lookup_any(|meta| {
            let q_enable = q_enable(meta);
            let mut constraints = vec![];

            let mut rlc = meta.query_advice(acc, Rotation::cur());
            let acc_mult = meta.query_advice(acc_mult, Rotation::cur());

            // If branch placeholder in S, value is 3 above.
            let rot_val = -3;

            let s_rlp1 = meta.query_advice(s_rlp1, Rotation(rot_val));
            rlc = rlc + s_rlp1 * acc_mult.clone();

            let s_rlp2 = meta.query_advice(s_rlp2, Rotation(rot_val));
            rlc = rlc + s_rlp2 * acc_mult.clone() * r_table[0].clone();

            let mut rind = 1;
            let mut r_wrapped = false;
            for col in s_advices.iter() {
                let s = meta.query_advice(*col, Rotation(rot_val));
                if !r_wrapped {
                    rlc = rlc + s * acc_mult.clone() * r_table[rind].clone();
                } else {
                    rlc = rlc
                        + s * acc_mult.clone()
                            * r_table[rind].clone()
                            * r_table[R_TABLE_LEN - 1].clone();
                }
                if rind == R_TABLE_LEN - 1 {
                    rind = 0;
                    r_wrapped = true;
                } else {
                    rind += 1;
                }
            }

            // Any rotation that lands into branch children can be used.
            let rot = -17;
            let is_branch_s_placeholder = meta.query_advice(
                s_advices[IS_BRANCH_S_PLACEHOLDER_POS - LAYOUT_OFFSET],
                Rotation(-23),
            );

            constraints.push((
                q_enable.clone() * rlc * is_branch_s_placeholder.clone(),
                meta.query_fixed(keccak_table[0], Rotation::cur()),
            ));

            for (ind, column) in s_keccak.iter().enumerate() {
                // placeholder branch contains hash of a leaf that moved to added branch
                let s_keccak = meta.query_advice(*column, Rotation(rot));
                let keccak_table_i =
                    meta.query_fixed(keccak_table[ind + 1], Rotation::cur());
                constraints.push((
                    q_enable.clone()
                        * s_keccak
                        * is_branch_s_placeholder.clone(),
                    keccak_table_i,
                ));
            }

            constraints
        });

        meta.lookup_any(|meta| {
            let q_enable = q_enable(meta);
            let mut constraints = vec![];

            let mut rlc = meta.query_advice(acc, Rotation::cur());
            let acc_mult = meta.query_advice(acc_mult, Rotation::cur());

            // If branch placeholder in C, value is 1 above.
            let rot_val = -1;

            let s_rlp1 = meta.query_advice(s_rlp1, Rotation(rot_val));
            rlc = rlc + s_rlp1 * acc_mult.clone();

            let s_rlp2 = meta.query_advice(s_rlp2, Rotation(rot_val));
            rlc = rlc + s_rlp2 * acc_mult.clone() * r_table[0].clone();

            let mut rind = 1;
            let mut r_wrapped = false;
            for col in s_advices.iter() {
                let s = meta.query_advice(*col, Rotation(rot_val));
                if !r_wrapped {
                    rlc = rlc + s * acc_mult.clone() * r_table[rind].clone();
                } else {
                    rlc = rlc
                        + s * acc_mult.clone()
                            * r_table[rind].clone()
                            * r_table[R_TABLE_LEN - 1].clone();
                }
                if rind == R_TABLE_LEN - 1 {
                    rind = 0;
                    r_wrapped = true;
                } else {
                    rind += 1;
                }
            }

            // Any rotation that lands into branch children can be used.
            let rot = -17;
            let is_branch_c_placeholder = meta.query_advice(
                s_advices[IS_BRANCH_C_PLACEHOLDER_POS - LAYOUT_OFFSET],
                Rotation(-23),
            );

            constraints.push((
                q_enable.clone() * rlc * is_branch_c_placeholder.clone(),
                meta.query_fixed(keccak_table[0], Rotation::cur()),
            ));

            for (ind, column) in c_keccak.iter().enumerate() {
                // placeholder branch contains hash of a leaf that moved to added branch
                let c_keccak = meta.query_advice(*column, Rotation(rot));
                let keccak_table_i =
                    meta.query_fixed(keccak_table[ind + 1], Rotation::cur());
                constraints.push((
                    q_enable.clone()
                        * c_keccak
                        * is_branch_c_placeholder.clone(),
                    keccak_table_i,
                ));
            }

            constraints
        });

        config
    }

    pub fn construct(config: LeafKeyInAddedBranchConfig) -> Self {
        Self {
            config,
            _marker: PhantomData,
        }
    }
}

impl<F: FieldExt> Chip<F> for LeafKeyInAddedBranchChip<F> {
    type Config = LeafKeyInAddedBranchConfig;
    type Loaded = ();

    fn config(&self) -> &Self::Config {
        &self.config
    }

    fn loaded(&self) -> &Self::Loaded {
        &()
    }
}
