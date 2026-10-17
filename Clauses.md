# Concept for SAT-Reduction

The encoding is implemented in `backend/src/encoding.rs` and works on `common::Board`, which
computes the possible bridges (`Board::edges`: pairs of islands that see each other, every island
only sees the nearest island in each direction) and which of them cross (`Board::crossings`).

**Variables:** two per possible bridge $e$ (numbered from 0):

- $x_{e,1} = 2e + 1$: at least one bridge is built on $e$,
- $x_{e,2} = 2e + 2$: a second bridge is built on $e$.

`api::to_dimacs` names every variable in a comment, e.g. `c var 1 = bridge (0,0)-(2,0) #1`.
Coordinates are `(x, y) = (column, row)`.

**Overview** (details below):

| Rule | Clauses |
|-|-|
| 1 Degree | exactly $k$ of the bridge variables of an island with number $k$ are true |
| Symmetry breaking | $x_{e,2} \rightarrow x_{e,1}$ |
| 2 No crossing | $\neg x_{e,1} \lor \neg x_{f,1}$ for crossing $e, f$ |
| 3 Connectivity (static) | cut edges of the graph of possible bridges are used; two 1s / two 2s with a double bridge can't be joined (more than 2 islands) |
| 4 Connectivity (lazy) | for every component of a disconnected model: one of the possible bridges leaving it is used |

## Rule 1

Rule 1.1: There are no more than two bridges in the same direction  
Rule 1.2: The number on an island must match its outgoing bridges

### variables

We chose the variables to be the possible edges between two connectable islands, two per edge
(see above), so an island with $m$ possible bridges has $n = 2m \le 8$ variables.

### clauses

We chose clauses to be the restrictions per island.

### Assumtions

The API validates puzzles (size, numbers 1 to 8, islands inside the grid); everything else, e.g.
unsolvable numbers, is handled by the encoding.

### Cases

#### Bridges with eight possible Bridges

We count bridges using $\lor$ for the upper and lower bound. This way we can force exactly $k$ out of $n$ variables to be true:

$$
\begin{align*}
\text{lower bound}\\
&\binom{n}{n-k+1}\text{combinations} \Rightarrow \text{clause width} = n-k+1:\\
\text{upper bound}\\
&\binom{n}{k+1}\text{negated combinations} \Rightarrow \text{clause width} = k+1:\\
\end{align*}
$$

This results in $\binom{n}{n-k+1} + \binom{n}{k+1}$ clauses, at most 112 for $n = 8$. This is much better than our previous approach (see commit history).

If an island needs more bridges than it has variables ($k > n$, e.g. a 5 with two neighbours or an
island without any neighbour) the rule produces the empty clause, so the puzzle is unsatisfiable.
(The old encoder produced satisfiable clauses in this case.)

#### Examples

##### Island with Number 3

```
(3)=
||
```

Assuming possible edges are labled from `a` to `d`:

$$
\begin{align*}
\text{At least 3 prop variables in positive polarity}\\
(a \lor b) \land\\
(a \lor c) \land\\
(a \lor d) \land\\
(b \lor c) \land\\
(b \lor d) \land\\
(c \lor d) \land\\
\text{At least 1 prop variables in negative polarity}\\
(\neg a \lor \neg b \lor \neg c \lor \neg d)
\end{align*}
$$

Please find more examples in tests.

##### Island with number 2 and island with number 1

Rule 1.1: There are no more than two bridges in the same direction.

    Example: If you have an island A and an island B, you can have at most two bridges connecting A to B, and at most two bridges connecting B to A.

Rule 1.2: The number on an island must match its outgoing bridges.

    Example: If an island has a number "2" on it, it means that there must be exactly two bridges connecting to other islands from that island.

Putting both rules together:

    Island A has the number "2" on it. This means there are exactly two bridges going out from Island A.
    Island B is connected to Island A with two bridges.
    Island B has the number "1" on it. This means there is exactly one bridge going out from Island B.
    Island C is connected to Island B with one bridge.
```
A ----- B ----- C
```
TODO: do the same as in the example above for variables.
TODO: variations of this f.e connect a to a D below or B to an E below (by incrementing the number of course)


## Symmetry breaking

Without further constraints a single bridge on $e$ could be represented by $x_{e,1}$ or by
$x_{e,2}$, so a solution with $b$ single bridges had $2^b$ models. We add

$$\neg x_{e,2} \lor x_{e,1}$$

for every edge: the second bridge implies the first one. Now every bridge configuration
corresponds to exactly one model, which is essential for counting solutions (blocking one
solution with a single clause, see "Uniqueness").

## Rule 2

2.1 Bridges don't cross other bridges  
2.2 Brdiges don't cross islands

**Current encoding:** 2.2 holds by construction (`Board` only creates edges between islands that
see each other). For 2.1, `Board::crossings` lists the crossing pairs and thanks to the symmetry
breaking ($x_{e,2} \rightarrow x_{e,1}$) a single clause per crossing pair suffices:

$$\neg x_{e,1} \lor \neg x_{f,1}$$

The derivation below is the original reasoning, the old encoder emitted the four clauses
$(\neg a_1 \lor \neg b_1), (\neg a_1 \lor \neg b_2), (\neg a_2 \lor \neg b_1), (\neg a_2 \lor \neg b_2)$.

We start by collecting all Islands that cross each other by checking for each bridge (Assuming possible bridges are stored left to right and top to bottom):  

Separate all bridges into vertical and horizontal:

Iterate over vertical subsets $A$:  
Iterate over horizontal subsets $B$:

Check for $((x_{a_{\text{from}}}, y_{a_{\text{from}}}), (x_{a_{\text{to}}}, y_{a_{\text{to}}})) \in A$ and $((x_{b_{\text{from}}}, y_{b_{\text{from}}}), (x_{b_{\text{to}}}, y_{b_{\text{to}}})) \in B$:

$$
\begin{align*}
(y_{a_{\text{from}}} < y_{b_{\text{from}}} = y_{b_{\text{to}}} < y_{a_{\text{to}}}) \land
(x_{a_{\text{from}}} < x_{b_{\text{from}}} = x_{b_{\text{to}}} < x_{a_{\text{to}}})
\end{align*}
$$

All edges $(a,b)$ where the above check results in true can be converted to clausels in the form of $a \oplus b$ and then be conjucted. Considering two bridges for each $a$ and $b$ we can expand the formula to $(a_1 \lor a_2) \oplus (b_1 \lor b_2) \oplus (\neg a_1 \land \neg a_2 \land \neg b_1 \land \neg b_2)$

resolve xor to CNF:
$$
\begin{align}
  &a \oplus b\\
  &\Rightarrow (a \land \neg b) \lor (\neg a \land b)\\
  &\Rightarrow_{\text{dist}} ((a \land \neg b) \lor \neg a) \land ((a \land \neg b) \lor b)\\
  &\Rightarrow_{\text{dist}} ((a \lor \neg a) \land (\neg b \lor \neg a)) \land ((a \lor b) \land (\neg b \lor b))\\
  &\Rightarrow_{\text{res}} (\neg a \lor \neg b) \land (a \lor b)
\end{align}
$$

Above constraint in CNF:

$$
\begin{align}
&(a_1 \lor \neg a_2 \lor b_1 \lor \neg b_2) \land\\
&(a_1 \lor \neg a_2 \lor \neg b_1 \lor b_2) \land\\
&(a_1 \lor \neg a_2 \lor \neg b_1 \lor \neg b_2) \land\\
&(\neg a_1 \lor a_2 \lor b_1 \lor \neg b_2) \land\\
&(\neg a_1 \lor a_2 \lor \neg b_1 \lor b_2) \land\\
&(\neg a_1 \lor a_2 \lor \neg b_1 \lor \neg b_2) \land\\
&(\neg a_1 \lor \neg a_2 \lor b_1 \lor \neg b_2) \land\\
&(\neg a_1 \lor \neg a_2 \lor \neg b_1 \lor b_2) \land\\
&(\neg a_1 \lor \neg a_2 \lor \neg b_1 \lor \neg b_2)
\end{align}
$$


## Rule 3

3.1: All Islands must be connected  
3.2: Cycles are possible

Connectivity is a global property that can't be expressed compactly in CNF. The static encoding
only contains *necessary* conditions (3a-3c), sufficiency is enforced lazily (Rule 4).

### 3a Cut edges

We view the puzzle as an undirected graph considering all possible bridges as edges and islands
as nodes. Using Tarjan's algorithm we look for *bridges* of this graph (called cut edges here to
avoid confusion with Hashi bridges): edges whose removal disconnects the graph. Every connected
solution has to use them, so we add the unit clause $x_{e,1}$ for each cut edge.

### 3b Unreachable islands

If the graph of possible bridges is not connected (some island can't reach the others, or an
island has no possible bridge at all), the puzzle is unsatisfiable and we add the empty clause.

### 3c Isolation

With more than two islands:

- two islands that both need exactly one bridge can't be joined: $\neg x_{e,1}$,
- two islands that both need two bridges can't be joined by a double bridge: $\neg x_{e,2}$,

because the two islands would form a closed group.

### Removed: the all-2 special case

The old encoder handled puzzles where every island has the number 2 by repeatedly removing edges
from the graph and then forcing the cut edges of the reduced graph. This is **unsound**: the
forced edges are arbitrary and may exclude the only solution. It was removed; Rule 4 handles
connectivity correctly for all puzzles.

## Rule 4: Lazy connectivity (CEGAR)

`backend/src/api.rs` solves in rounds:

1. Solve the CNF (Rules 1-3 plus all clauses added so far).
2. UNSAT: the puzzle has no solution. SAT: decode the bridges and compute the connected
   components of the islands using only the built bridges.
3. One component: the model is a solution. Otherwise add, for every component $C$, the clause

   $$\bigvee_{e \text{ leaves } C} x_{e,1}$$

   over the possible bridges with exactly one end in $C$ ("one of the bridges leaving $C$ is
   used"), which every connected solution satisfies but the current model doesn't, and go to 1.
   With two components both clauses are the same, so only one is added.

Every round excludes the current model, so the loop terminates. `SolveStats` reports the number
of SAT calls (`sat_calls`) and of added cut clauses (`connectivity_cuts`). The whole loop respects
the timeout of the solver configuration.

## Uniqueness

`api::solve_unique` blocks the found solution with the clause

$$\bigvee_{e} \left(x_{e,1} \ne s_{e,1}\right) \lor \left(x_{e,2} \ne s_{e,2}\right)$$

(the negation of the model, which thanks to the symmetry breaking excludes exactly this bridge
configuration) and runs the connectivity loop again: UNSAT means the solution is unique.

## Example: only disconnected solutions

```
1...1
.....
2...2
.....
1...1
```

Rules 1-3 are satisfiable (the 1s go to their 2, both columns are closed groups), but 8 bridge
ends make only 4 bridges, too few to connect 6 islands. Rule 4 adds the cut "one of the bridges
leaving the left column is used"; the only candidate is the 2-2 bridge in the middle (the 1-1
bridges are excluded by 3c), which would give the 2s three bridges, so the puzzle is UNSAT.
