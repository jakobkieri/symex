use boolector::{
    option::{BtorOption, ModelGen, NumberFormat},
    Btor, SolverResult,
};
use std::rc::Rc;

use super::{Solutions, SolverError, BV};

#[derive(Debug, Clone)]
pub struct Solver {
    btor: Rc<Btor>,
}

impl Default for Solver {
    fn default() -> Self {
        Self::new()
    }
}

impl Solver {
    pub fn new() -> Self {
        let btor = Btor::new();
        btor.set_opt(BtorOption::Incremental(true));
        btor.set_opt(BtorOption::PrettyPrint(true));
        btor.set_opt(BtorOption::OutputNumberFormat(NumberFormat::Hexadecimal));

        Self {
            btor: Rc::new(btor),
        }
    }

    /// Check if it is satisfiable.
    pub fn is_sat(&self) -> Result<bool, SolverError> {
        match self.btor.sat() {
            SolverResult::Sat => Ok(true),
            SolverResult::Unsat => Ok(false),
            SolverResult::Unknown => Err(SolverError::Unknown),
        }
    }

    /// Check if sat with the extra constraint.
    pub fn is_sat_with_constraint(&self, constraint: &BV) -> Result<bool, SolverError> {
        // Can't i use assume here?
        // it should only be valid until the next call to is_sat

        self.btor.push(1);
        self.assert(constraint);
        let is_sat = self.is_sat()?;
        self.btor.pop(1);
        Ok(is_sat)
    }

    pub fn assert(&self, bv: &BV) {
        bv.0.assert();
    }

    // -------------------------------------------------------------------------
    // BV helpers
    // -------------------------------------------------------------------------

    // /// Returns `true` if `lhs` and `rhs` must be equal under the current
    // /// constraints.
    // pub fn bvs_must_be_equal(&self, lhs: &BV, rhs: &BV) -> Result<bool> {
    //     // Add the constraint lhs != rhs and invert the results. The only way
    //     // for `lhs != rhs` to be `false` is that if they are equal.
    //     let constraint = lhs._ne(&rhs);
    //     let result = self.is_sat_with_constraint(&constraint)?;
    //     Ok(!result)
    // }

    /// Check if `lhs` and `rhs` can be equal under the current constraints.
    pub fn can_equal(&self, lhs: &BV, rhs: &BV) -> Result<bool, SolverError> {
        self.is_sat_with_constraint(&lhs.eq(&rhs))
    }

    pub fn get_solutions_for_bv(
        &self,
        bv: &BV,
        max_solutions: usize,
    ) -> Result<Solutions, SolverError> {
        // Setup before checking for solutions.
        self.btor.push(1);
        self.btor.set_opt(BtorOption::ModelGen(ModelGen::All));

        let result = self.internal_get_solutions_for_bv(bv, max_solutions);

        // Restore solver to initial state.
        self.btor.set_opt(BtorOption::ModelGen(ModelGen::Disabled));
        self.btor.pop(1);

        result
    }

    /// Helper to ensure we always set `ModelGen::Disabled` for all paths in this function.
    fn internal_get_solutions_for_bv(
        &self,
        bv: &BV,
        max_solutions: usize,
    ) -> Result<Solutions, SolverError> {
        if !self.is_sat()? {
            return Ok(Solutions::None);
        }

        let mut solutions = Vec::new();
        while solutions.len() < max_solutions && self.is_sat()? {
            let solution = bv.get_solution().disambiguate();

            // Constrain the next value to not be an already found solution.
            let solution_bv = self.from_binary_string(solution.as_01x_str());
            self.assert(&bv.ne(&solution_bv));

            solutions.push(solution);
        }

        if solutions.len() == 0 {
            Ok(Solutions::None)
        } else {
            let exists_more_solutions = self.is_sat()?;
            match exists_more_solutions {
                false => Ok(Solutions::Exactly(solutions)),
                true => Ok(Solutions::AtLeast(solutions)),
            }
        }
    }

    // -------------------------------------------------------------------------
    // Incremental stuff
    // -------------------------------------------------------------------------

    pub fn push(&self, n: u32) {
        self.btor.push(n)
    }
    pub fn pop(&self, n: u32) {
        self.btor.pop(n)
    }

    // -------------------------------------------------------------------------
    // BV creation
    // -------------------------------------------------------------------------

    pub fn array(
        &self,
        index_width: u32,
        element_width: u32,
        symbol: Option<&str>,
    ) -> boolector::Array<Rc<Btor>> {
        boolector::Array::new(self.btor.clone(), index_width, element_width, symbol)
    }

    pub fn bv(&self, bits: u32) -> BV {
        boolector::BV::new(self.btor.clone(), bits, None).into()
    }

    pub fn bv_from_bool(&self, value: bool) -> BV {
        boolector::BV::from_bool(self.btor.clone(), value).into()
    }

    /// Creates new bv from a u64.
    pub fn bv_from_u64(&self, value: u64, bits: u32) -> BV {
        boolector::BV::from_u64(self.btor.clone(), value, bits).into()
    }

    /// Gets a new bv with a zero value.
    pub fn bv_zero(&self, bits: u32) -> BV {
        boolector::BV::zero(self.btor.clone(), bits).into()
    }

    pub fn from_binary_string(&self, bits: &str) -> BV {
        boolector::BV::from_binary_str(self.btor.clone(), bits).into()
    }
}
