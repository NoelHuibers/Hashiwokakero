1260 runs of 7 solvers on 180 formulas; 511 solved both natively and in the sandbox with at least 20 ms natively are used for the correlations and the fits.

### Fuel against native time

| model | minisat | cadical | kissat | splr | cdcl | dpll | starter | pooled | RMS log error | held out: RMS | held out: bias spread |
|---|---|---|---|---|---|---|---|---|---|---|---|
| uniform | ρ 0.948 / τ 0.849 (n=56) | ρ 0.929 / τ 0.823 (n=83) | ρ 0.976 / τ 0.896 (n=106) | ρ 0.983 / τ 0.906 (n=93) | ρ 0.913 / τ 0.788 (n=57) | ρ 0.990 / τ 0.933 (n=74) | ρ 0.996 / τ 0.963 (n=42) | ρ 0.957 / τ 0.820 | 0.390 | 0.411 | 0.999 |
| mems | ρ 0.966 / τ 0.877 (n=56) | ρ 0.927 / τ 0.820 (n=83) | ρ 0.976 / τ 0.893 (n=106) | ρ 0.987 / τ 0.917 (n=93) | ρ 0.914 / τ 0.797 (n=57) | ρ 0.971 / τ 0.876 (n=74) | ρ 0.995 / τ 0.954 (n=42) | ρ 0.958 / τ 0.828 | 0.391 | 0.408 | 0.690 |
| leaderboard | ρ 0.949 / τ 0.853 (n=56) | ρ 0.928 / τ 0.821 (n=83) | ρ 0.976 / τ 0.894 (n=106) | ρ 0.981 / τ 0.905 (n=93) | ρ 0.913 / τ 0.789 (n=57) | ρ 0.987 / τ 0.919 (n=74) | ρ 0.995 / τ 0.958 (n=42) | ρ 0.963 / τ 0.835 | 0.361 | 0.372 | 0.828 |
| fitted, 3 groups | ρ 0.949 / τ 0.853 (n=56) | ρ 0.928 / τ 0.822 (n=83) | ρ 0.975 / τ 0.893 (n=106) | ρ 0.981 / τ 0.904 (n=93) | ρ 0.913 / τ 0.789 (n=57) | ρ 0.987 / τ 0.919 (n=74) | ρ 0.995 / τ 0.958 (n=42) | ρ 0.962 / τ 0.833 | 0.365 | 0.384 | 0.917 |
| fitted, 3 groups (table) | ρ 0.949 / τ 0.853 (n=56) | ρ 0.928 / τ 0.822 (n=83) | ρ 0.976 / τ 0.894 (n=106) | ρ 0.982 / τ 0.905 (n=93) | ρ 0.913 / τ 0.789 (n=57) | ρ 0.987 / τ 0.918 (n=74) | ρ 0.995 / τ 0.956 (n=42) | ρ 0.962 / τ 0.834 | 0.364 | 0.383 | 0.906 |
| fitted, per category | ρ 0.981 / τ 0.904 (n=56) | ρ 0.927 / τ 0.814 (n=83) | ρ 0.968 / τ 0.882 (n=106) | ρ 0.988 / τ 0.918 (n=93) | ρ 0.911 / τ 0.782 (n=57) | ρ 0.994 / τ 0.947 (n=74) | ρ 0.993 / τ 0.951 (n=42) | ρ 0.972 / τ 0.873 | 0.315 | 0.376 | 0.608 |
| fitted, per category (table) | ρ 0.981 / τ 0.903 (n=56) | ρ 0.929 / τ 0.825 (n=83) | ρ 0.971 / τ 0.886 (n=106) | ρ 0.987 / τ 0.919 (n=93) | ρ 0.912 / τ 0.787 (n=57) | ρ 0.994 / τ 0.945 (n=74) | ρ 0.995 / τ 0.958 (n=42) | ρ 0.973 / τ 0.874 | 0.312 | 0.367 | 0.537 |

Per-solver bias when the solver is left out (median log of time / predicted time; 0 is perfect, 0.69 is twice as slow as predicted):

| model | minisat | cadical | kissat | splr | cdcl | dpll | starter |
|---|---|---|---|---|---|---|---|
| uniform | +0.353 | +0.023 | +0.073 | +0.433 | -0.565 | -0.365 | -0.390 |
| mems | +0.409 | -0.108 | -0.151 | +0.522 | -0.168 | +0.013 | -0.103 |
| leaderboard | +0.318 | -0.034 | -0.009 | +0.449 | -0.379 | -0.187 | -0.214 |
| fitted, 3 groups | +0.336 | -0.094 | +0.035 | +0.459 | -0.458 | -0.160 | -0.218 |
| fitted, 3 groups (table) | +0.332 | -0.088 | +0.021 | +0.468 | -0.438 | -0.163 | -0.217 |
| fitted, per category | -0.106 | -0.201 | +0.355 | +0.205 | -0.149 | -0.133 | -0.252 |
| fitted, per category (table) | -0.188 | -0.211 | +0.294 | +0.210 | -0.221 | -0.119 | -0.243 |

### Weights

- uniform: load 1, store 1, alu 1, muldiv 1, float 1, branch 1, call 1, local 1, bulk 1, grow 1, other 1
- mems: load 1, store 1, alu 0, muldiv 0, float 0, branch 0, call 0, local 0, bulk 1, grow 0, other 0
- leaderboard: load 3, store 3, alu 1, muldiv 1, float 1, branch 1, call 1, local 0, bulk 1, grow 1, other 1
- fitted, 3 groups, ns per operation: load 0.185, store 0.185, alu 0.071, muldiv 0.071, float 0.071, branch 0.071, call 0.071, local 0.000, bulk 0.185, grow 0.071, other 0.071, function entry 0.071
- fitted, 3 groups (table): load 9, store 9, alu 3, muldiv 3, float 3, branch 3, call 3, local 0, bulk 9, grow 3, other 3
- fitted, per category, ns per operation: load 0.000, store 0.086, alu 0.008, muldiv 0.000, float 11.619, branch 0.229, call 0.000, local 0.025, bulk 0.000, grow 6540.459, other 0.000, function entry 0.480
- fitted, per category (table): load 0, store 2, alu 0, muldiv 0, float 255, branch 5, call 0, local 1, bulk 0, grow 255, other 0

### Rankings by PAR-2

Native time limit 566 ms (what the median run needs for the budget's uniform fuel); fuel budgets scaled the same way per model.

- native time (τ 1.000): minisat 0.158, cadical 0.195, kissat 0.210, splr 0.479, cdcl 1.168, starter 1.215, dpll 1.370
- fuel, uniform (τ 0.905): minisat 0.150, kissat 0.207, cadical 0.213, splr 0.435, cdcl 1.235, starter 1.243, dpll 1.428
- fuel, mems (τ 0.905): minisat 0.148, kissat 0.228, cadical 0.251, splr 0.421, cdcl 1.179, starter 1.227, dpll 1.377
- fuel, leaderboard (τ 0.905): minisat 0.152, kissat 0.212, cadical 0.219, splr 0.434, cdcl 1.204, starter 1.229, dpll 1.396
- fuel, fitted, 3 groups (τ 0.905): minisat 0.152, kissat 0.213, cadical 0.218, splr 0.427, cdcl 1.216, starter 1.230, dpll 1.402
- fuel, fitted, 3 groups (table) (τ 0.905): minisat 0.152, kissat 0.215, cadical 0.220, splr 0.422, cdcl 1.204, starter 1.229, dpll 1.395
- fuel, fitted, per category (τ 0.905): minisat 0.187, kissat 0.209, cadical 0.214, splr 0.484, cdcl 1.170, starter 1.231, dpll 1.404
- fuel, fitted, per category (table) (τ 0.905): minisat 0.178, kissat 0.210, cadical 0.214, splr 0.473, cdcl 1.178, starter 1.232, dpll 1.403

### Sandbox against native

| solver | median | quartiles |
|---|---|---|
| minisat | 1.46× | 1.43–1.64× |
| cadical | 1.70× | 1.61–1.91× |
| kissat | 1.62× | 1.56–1.73× |
| splr | 1.54× | 1.49–1.57× |
| cdcl | 1.74× | 1.58–1.85× |
| dpll | 1.65× | 1.50–1.81× |
| starter | 2.35× | 1.86–2.72× |

### Formulas

- coloring: 27
- factoring: 27
- hamiltonian: 4
- hashi: 19
- hashi-parity: 19
- miter: 23
- pigeonhole: 19
- quasigroup: 4
- queens: 15
- random-3sat: 19
- tseitin: 4
