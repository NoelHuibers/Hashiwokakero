# Concept for SAT-Reduction

## Rule 1

Rule 1.1: There are no more than two bridges in the same direction  
Rule 1.2: The number on an island must match its outgoing bridges

### variables

We chose the variables to be the possible edges between two connectable islands.

### clauses

We chose clauses to be the restrictions per island.

### Assumtions

We assume to always receive valid games that match the ruleset of the game.

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

All edges $(a,b)$ where the above check results in true can be converted to clausels in the form of $a \oplus b$ and then be conjucted. Considering two bridges for each $a$ and $b$ we can expand the formula to $(a_1 \oplus b_1) \land (a_2 \oplus b_1) \land (a_1 \oplus b_2) \land (a_2 \oplus b_2)$

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


## Rule 3

3.1: All Islands must be connected  
3.2: Cycles are possible

A Graph of $n$ Islands needs at least $n-1$ unique bridges to be connected (based on a spanning tree). Assuming a pair of possible Bridges is called $a, b$ we can use $\lor$ as a constraint for at least one Bridge. We demand at least one unique bridge for at least $n-1$ nodes to make sure every island is connected.



### Example
```
(1)==(2)
||    ||
||    ||
(1)==(2)
```

$n = 4$  
$n-1 = 3$  

Bridges are labelled after start-to-end indices starting with coordinate 00 in the left upper corner.

Concunction of Disjunctions:

$$
\begin{align*}
(("00|10|0" \lor "00|10|1") \lor
("00|01|0" \lor "00|01|1") \lor
("10|11|0" \lor "10|11|1") \lor
("01|11|0" \lor "01|11|1")) \land\\
(("00|10|0" \lor "00|10|1") \lor
("00|01|0" \lor "00|01|1") \lor
("10|11|0" \lor "10|11|1")) \land\\
(("00|01|0" \lor "00|01|1") \lor
("10|11|0" \lor "10|11|1")) \land\\
\end{align*}
$$
