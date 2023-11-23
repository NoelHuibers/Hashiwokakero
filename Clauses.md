# Concept for SAT-Reduction

## Rule 1

Rule 1.1: There are no more than two bridges in the same direction  
Rule 1.2: The number on an island must match its outgoing bridges

### variables

We chose the variables to be the possible edges between two connectable islands.

### clauses

We chose clauses to be the restrictions per island.

### Assumtions

We assume to always receive valid games that match the rule set of the game.

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

This results in $k$ clauses with a maximum size of $k \cdot (k-1)$. This is much better than our previous approach (see commit history).

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


## Rule 2

2.1 Bridges don't cross other bridges  
2.2 Brdiges don't cross islands

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

We view the puzzle as an undirected graph considering all possible bridges as edges and islands as nodes. We look for bridges in the graph using Tarjans algorithm. As bridges are the fragile edges of the graph where we definitely force a bridge by adding the bridge pair in a clause.

However, finding bridges in the possible edges we collect is not enough. There are two special cases we need to consider:

1. Two disconnected island groups where no edge would be possible (invalid game)
2. Puzzles where every island has exactly the number 2 (special case as max amount of bridges is 2 as well)

As the first game is an invalid game, we check the reached edges from our iteration of the Tarjan algorithm and force empty clauses if the dfs in Tarjan results in unconnectedness.

For the second case we first thought of banning double bridges. But cases like shown below would still fail:

```
2-2
| |
2-2

2-2
| |
2-2
```

So in the case of having only islands with number two we consecutively remove those edges in the graph whose removal doesn't lead to new bridges. We do that until there is no such edge any more. In that case we remove an arbitrary edge and go back to our base case finding bridges and forcing them through constraints.

### Examples

Let's consider a simple example of a Hashi puzzle with four islands:

    Island A has the number "1" on it.
    Island B has the number "3" on it.
    Island C has the number "1" on it.
    Island D has the number "2" on it.
    Island E has the number "1" on it.

Here's a possible solution that adheres to the connectivity rules:
```
A ----- B ----- C
        |
        |
E ----- D
```

Prove connectivity by "Cut Vertices (Articulation Points)":
  1. Perform Depth-First Search (DFS)
  2. Identify Back Edges
  3. Lowest Reachable Ancestor
  4. Cut vertices
  5. Bridges