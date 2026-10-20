# Hashiwokakero SAT encoding

The encoding is implemented in [backend/src/encoding.rs](backend/src/encoding.rs).
[common::Board](common/src/board.rs) computes possible bridges between the nearest
visible islands in each row and column, and identifies crossing pairs.
The [backend API](backend/src/api.rs) validates puzzles before encoding them.

## Variables

Each possible bridge edge $e$, numbered from zero, has two Boolean variables:

- $x_{e,1} = 2e + 1$: at least one bridge is built on $e$.
- $x_{e,2} = 2e + 2$: a second bridge is built on $e$.

The clause

$$\neg x_{e,2} \lor x_{e,1}$$

ensures that a second bridge implies the first. The assignments $(0,0)$, $(1,0)$,
and $(1,1)$ therefore represent zero, one, and two bridges. Each bridge
configuration has exactly one representation.

`api::to_dimacs` exports the static CNF with comments naming each variable and its
endpoints. Coordinates are zero-based `(x, y) = (column, row)`.

## Island degrees

An island requiring $k$ bridges must have exactly $k$ true incident variables.
For $m$ possible incident edges there are $n = 2m \le 8$ variables.

- **At least $k$:** for every subset of $n-k+1$ variables, add their disjunction.
- **At most $k$:** for every subset of $k+1$ variables, add the disjunction of their negations.

For $1 \le k \le n$, this produces

$$\binom{n}{n-k+1} + \binom{n}{k+1}$$

clauses, at most 112 when $n \le 8$. If $k > n$, an empty clause makes the formula
unsatisfiable. The helper also supports $k = 0$ by negating each variable.

For example, exactly three of $a,b,c,d$ must be true:

$$
(a \lor b) \land (a \lor c) \land (a \lor d) \land
(b \lor c) \land (b \lor d) \land (c \lor d) \land
(\neg a \lor \neg b \lor \neg c \lor \neg d).
$$

## No crossings

For each pair of crossing edges $e,f$, add

$$\neg x_{e,1} \lor \neg x_{f,1}.$$

Because a second bridge implies the first, this excludes all crossing bridge
counts. Bridges cannot pass through islands because possible edges only join
nearest visible neighbours.

## Static connectivity constraints

The static encoding includes necessary conditions for connectivity:

- If the graph of possible bridges is disconnected, add an empty clause.
- Find its cut edges with Tarjan's algorithm. Every connected solution must use
  these edges, so add $x_{e,1}$ for each one.
- When there are more than two islands, prohibit an edge between two degree-one
  islands with $\neg x_{e,1}$, and a double bridge between two degree-two islands
  with $\neg x_{e,2}$. Either would isolate those two islands from the others.

These constraints alone do not guarantee connectivity. Exported static CNF needs
the connectivity check below to yield a valid puzzle solution. Cycles are allowed.

## Lazy connectivity

The backend solves in rounds:

1. Solve the current CNF. Return unsatisfiable or timeout if appropriate.
2. Decode a SAT model into bridges and compute the connected components.
3. Return the solution if all islands are connected.
4. Otherwise, for each component $C$, add a cut clause and solve again:

   $$\bigvee_{e \text{ leaves } C} x_{e,1}.$$

A cut includes all possible edges with exactly one endpoint in $C$. Every connected
solution must use at least one of them. The clause excludes the disconnected
model while preserving every valid solution. With two components, their cuts are
identical, so only one is added.

Each round excludes the current model, so the search terminates if allowed to
finish. All rounds share the solver's time limit. `SolveStats` reports SAT calls
and added cuts as `sat_calls` and `connectivity_cuts`.

## Uniqueness

After finding a solution, `api::solve_unique` adds one clause that negates its
complete variable assignment, excluding exactly that bridge configuration. It
then repeats the connectivity search:

- Unsatisfiable: the first solution is unique.
- Another connected solution: the puzzle has multiple solutions.
- Timeout: uniqueness is unknown.

The uniqueness check shares the original solving deadline.
