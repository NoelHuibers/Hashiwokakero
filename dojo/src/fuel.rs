//! What fuel costs: operator categories and the cost tables built from them.
//!
//! wasmtime charges every WebAssembly operator a cost from a table (`OperatorCost`, one `u8` per
//! operator) plus per-byte and per-page costs for bulk memory operations and `memory.grow`.
//! Its default charges 1 for every operator, 0 for those that generate no code (`nop`,
//! `drop`, `block`, `loop`, `end`, `else`, `return`, `unreachable`), and 1 per byte or page.
//!
//! On top of the table wasmtime charges one unit for every function entry, whatever the table
//! says (its instrumentation starts every function with one unit consumed).
//!
//! The Dojo groups operators into [`Category`]s and gives every operator of a category the same
//! cost ([`Weights`]). With one weight per category the calibration study (`dojo calibrate`,
//! dojo/CALIBRATION.md) measures how often a solver executes each kind of operator: a table
//! that charges one category only counts that category (plus the function entries, which a
//! table of zeros counts alone, see [`Counts`]).

use wasmtime::{OperatorCost, VariableOperatorCost};

/// Kinds of operators, by what they cost a real machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    /// Loads from linear memory.
    Load,
    /// Stores to linear memory.
    Store,
    /// Integer arithmetic, logic, comparisons, conversions and `select`.
    Alu,
    /// Integer multiplication, division and remainder.
    MulDiv,
    /// Floating point operations and conversions.
    Float,
    /// Branches: `br`, `br_if`, `br_table`, `if`.
    Branch,
    /// Calls, direct and indirect.
    Call,
    /// Locals, globals and constants (mostly registers once compiled).
    Local,
    /// `memory.copy`, `memory.fill`, `memory.init` and the table versions: per operation and per
    /// byte or element.
    Bulk,
    /// `memory.grow`, per operation and per page.
    Grow,
    /// Everything else: SIMD, reference types, `memory.size`, table access.
    Other,
}

impl Category {
    pub const ALL: [Self; 11] = [
        Self::Load,
        Self::Store,
        Self::Alu,
        Self::MulDiv,
        Self::Float,
        Self::Branch,
        Self::Call,
        Self::Local,
        Self::Bulk,
        Self::Grow,
        Self::Other,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Store => "store",
            Self::Alu => "alu",
            Self::MulDiv => "muldiv",
            Self::Float => "float",
            Self::Branch => "branch",
            Self::Call => "call",
            Self::Local => "local",
            Self::Bulk => "bulk",
            Self::Grow => "grow",
            Self::Other => "other",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.id() == id)
    }

    fn index(self) -> usize {
        self as usize
    }

    /// The category of an operator by its name in wasmparser (`I32Load`, `BrIf`, ...), `None`
    /// for those that generate no code and are always free, like in wasmtime's default.
    pub fn of(op: &str) -> Option<Self> {
        let int = op.starts_with("I32") || op.starts_with("I64");
        let float = op.starts_with("F32") || op.starts_with("F64");
        Some(match op {
            "Nop" | "Drop" | "Block" | "Loop" | "End" | "Else" | "Return" | "Unreachable" => {
                return None;
            }
            "MemoryGrow" => Self::Grow,
            "MemoryCopy" | "MemoryFill" | "MemoryInit" | "DataDrop" | "TableCopy" | "TableFill"
            | "TableInit" | "ElemDrop" => Self::Bulk,
            "Call" | "CallIndirect" | "CallRef" | "ReturnCall" | "ReturnCallIndirect"
            | "ReturnCallRef" => Self::Call,
            "Br" | "BrIf" | "BrTable" | "If" | "BrOnNull" | "BrOnNonNull" => Self::Branch,
            "LocalGet" | "LocalSet" | "LocalTee" | "GlobalGet" | "GlobalSet" | "I32Const"
            | "I64Const" | "F32Const" | "F64Const" => Self::Local,
            "Select" | "TypedSelect" => Self::Alu,
            _ if op.contains("Load") => Self::Load,
            _ if op.contains("Store") => Self::Store,
            _ if int && (op.contains("Mul") || op.contains("Div") || op.contains("Rem")) => {
                Self::MulDiv
            }
            _ if float || (int && (op.contains("Trunc") || op.contains("Reinterpret"))) => {
                Self::Float
            }
            _ if int => Self::Alu,
            _ => Self::Other,
        })
    }
}

/// How often a run executed each [`Category`] (operators, bytes and pages as the table charges
/// them) and how many functions it entered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Counts {
    pub ops: [u64; Category::ALL.len()],
    pub entries: u64,
}

impl Counts {
    pub fn get(&self, category: Category) -> u64 {
        self.ops[category.index()]
    }
}

/// The cost model of the leaderboard, calibrated against native running time.
///
/// See dojo/CALIBRATION.md. Loads and stores cost three units, locals, globals and constants
/// nothing (they live in registers once compiled), every other operator one, bulk memory one
/// per byte and `memory.grow` one per page. Function entries cost one, like in every table.
pub const MODEL: Weights = Weights([3, 3, 1, 1, 1, 1, 1, 0, 1, 1, 1]);

/// A cost per [`Category`]: what every operator of the category costs, and every byte (or
/// element) of a bulk operation or page of `memory.grow`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Weights(pub [u8; Category::ALL.len()]);

impl Weights {
    /// wasmtime's default: one unit per operator, byte and page.
    pub const UNIFORM: Self = Self([1; Category::ALL.len()]);

    /// Every operator free: such a table only counts the function entries.
    pub const ZERO: Self = Self([0; Category::ALL.len()]);

    /// Only `category` costs (one unit), everything else is free: counts that category (and
    /// the function entries).
    pub fn only(category: Category) -> Self {
        let mut weights = Self::ZERO;
        weights.0[category.index()] = 1;
        weights
    }

    pub fn get(&self, category: Category) -> u8 {
        self.0[category.index()]
    }

    pub fn set(&mut self, category: Category, weight: u8) {
        self.0[category.index()] = weight;
    }

    /// The fuel of a run from how often it executed each category.
    pub fn fuel(&self, counts: &Counts) -> u64 {
        Category::ALL
            .iter()
            .map(|&c| u64::from(self.get(c)).saturating_mul(counts.ops[c.index()]))
            .fold(counts.entries, u64::saturating_add)
    }

    /// The cost table for wasmtime.
    pub fn table(&self) -> OperatorCost {
        let mut table = OperatorCost::new();
        fill(&mut table, |op| {
            Category::of(op).map_or(0, |category| self.get(category))
        });
        let bulk = self.get(Category::Bulk);
        let other = self.get(Category::Other);
        table.variable = VariableOperatorCost {
            memory_copy_per_byte: bulk,
            memory_fill_per_byte: bulk,
            memory_init_per_byte: bulk,
            memory_grow_per_page: self.get(Category::Grow),
            table_copy_per_element: bulk,
            table_fill_per_element: bulk,
            table_init_per_element: bulk,
            table_grow_per_element: other,
            array_copy_per_element: other,
            array_fill_per_element: other,
            array_new_data_per_element: other,
            array_init_data_per_element: other,
            array_new_elem_per_element: other,
            array_init_elem_per_element: other,
            array_new_default_per_element: other,
            array_new_per_element: other,
        };
        table
    }
}

/// Defines [`fill`] with every operator wasmtime knows (the list of its own wasmparser, the
/// one `OperatorCost` is made from).
macro_rules! define_fill {
    ($(@$proposal:ident $op:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*) )*) => {
        /// Sets the cost of every operator to `cost(its name)`.
        fn fill(table: &mut OperatorCost, cost: impl Fn(&str) -> u8) {
            $( table.$op = cost(stringify!($op)); )*
        }

        /// The names of all operators.
        #[cfg(test)]
        pub(crate) const OPERATORS: &[&str] = &[$(stringify!($op)),*];
    };
}
wasmtime::wasmparser::for_each_operator!(define_fill);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operators_fall_into_categories() {
        for (op, category) in [
            ("I32Load8U", Some(Category::Load)),
            ("F64Load", Some(Category::Load)),
            ("I64Store32", Some(Category::Store)),
            ("I32Add", Some(Category::Alu)),
            ("I64Eqz", Some(Category::Alu)),
            ("I64ExtendI32S", Some(Category::Alu)),
            ("I32WrapI64", Some(Category::Alu)),
            ("Select", Some(Category::Alu)),
            ("I64Mul", Some(Category::MulDiv)),
            ("I32DivU", Some(Category::MulDiv)),
            ("I64RemS", Some(Category::MulDiv)),
            ("F32Mul", Some(Category::Float)),
            ("F64ConvertI64U", Some(Category::Float)),
            ("I32TruncSatF64U", Some(Category::Float)),
            ("I64ReinterpretF64", Some(Category::Float)),
            ("BrIf", Some(Category::Branch)),
            ("If", Some(Category::Branch)),
            ("CallIndirect", Some(Category::Call)),
            ("LocalTee", Some(Category::Local)),
            ("I64Const", Some(Category::Local)),
            ("MemoryFill", Some(Category::Bulk)),
            ("MemoryGrow", Some(Category::Grow)),
            ("MemorySize", Some(Category::Other)),
            ("I8x16Swizzle", Some(Category::Other)),
            ("End", None),
            ("Drop", None),
        ] {
            assert_eq!(Category::of(op), category, "{op}");
        }
        for category in Category::ALL {
            assert_eq!(Category::from_id(category.id()), Some(category));
        }
        assert_eq!(Category::from_id("nothing"), None);
    }

    /// The uniform table is wasmtime's default: what every score so far was computed with.
    #[test]
    fn uniform_is_wasmtime_default() {
        let table = Weights::UNIFORM.table();
        let default = OperatorCost::new();
        assert_eq!(table, default);
        assert!(OPERATORS.len() > 400, "{}", OPERATORS.len());
        // Only counting one category: nothing else costs
        let loads = Weights::only(Category::Load).table();
        assert_eq!((loads.I32Load, loads.I32Add, loads.Call), (1, 0, 0));
        assert_eq!(loads.variable.memory_fill_per_byte, 0);
        let bulk = Weights::only(Category::Bulk).table();
        assert_eq!(
            (bulk.MemoryFill, bulk.variable.memory_copy_per_byte),
            (1, 1)
        );
    }

    #[test]
    fn fuel_adds_up_the_categories() {
        let mut counts = Counts {
            entries: 2,
            ..Counts::default()
        };
        counts.ops[Category::Load.index()] = 10;
        counts.ops[Category::Alu.index()] = 5;
        assert_eq!(counts.get(Category::Load), 10);
        assert_eq!(Weights::UNIFORM.fuel(&counts), 17);
        let mut weights = Weights::UNIFORM;
        weights.set(Category::Load, 3);
        assert_eq!(weights.fuel(&counts), 37);
        assert_eq!(Weights::only(Category::Alu).fuel(&counts), 7);
        assert_eq!(Weights::ZERO.fuel(&counts), 2);
    }
}
