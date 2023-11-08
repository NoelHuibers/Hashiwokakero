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

We count bridges using $\lor$ for the upper and lower bound. This way we can force exactly $n$ out of $k$ variables to be true:

$$
\begin{align*}
\text{lower bound}\\
&(x_1 \lor \dots \lor x_k) \land\\
&(x_2 \lor \dots \lor x_k) \land\\
&\vdots\\
&(x_n \lor \dots \lor x_k) \land\\
\text{upper bound}\\
&(\neg x_1 \lor \dots \lor \neg x_k) \land\\
&(\neg x_2 \lor \dots \lor \neg x_k) \land\\
&\vdots\\
&(\neg x_{k-n} \lor \dots \lor \neg x_k) \land\\
\end{align*}
$$

This results in $k$ clauses with a maximum size of $k \cdot (k-1)$. This is much better than our previous approach (see commit history).

#### Examples

##### Island with Number 4

```
  ||
=(4)=
  ||
```

Assuming edges are labled from `a` to `h`:

$$
\begin{align*}
\text{At least 4 prop variables in positive polarity}\\
(a \lor b \lor c \lor d \lor e \lor f \lor g \lor h) \land\\
(b \lor c \lor d \lor e \lor f \lor g \lor h) \land\\
(c \lor d \lor e \lor f \lor g \lor h) \land\\
(d \lor e \lor f \lor g \lor h) \land\\
\text{At least 4 prop variables in negative polarity}\\
(\neg a \lor \neg b \lor \neg c \lor \neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
(\neg b \lor \neg c \lor \neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
(\neg c \lor \neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
(\neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
\end{align*}
$$

##### Island with Number 5

$$
\begin{align*}
\text{At least 5 prop variables in positive polarity}\\
(a \lor b \lor c \lor d \lor e \lor f \lor g \lor h) \land\\
(b \lor c \lor d \lor e \lor f \lor g \lor h) \land\\
(c \lor d \lor e \lor f \lor g \lor h) \land\\
(d \lor e \lor f \lor g \lor h) \land\\
(e \lor f \lor g \lor h)\\
\text{At least 3 prop variables in negative polarity}\\
(\neg a \lor \neg b \lor \neg c \lor \neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
(\neg b \lor \neg c \lor \neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
(\neg c \lor \neg d \lor \neg e \lor \neg f \lor \neg g \lor \neg h) \land\\
\end{align*}
$$

## Rule 2

2.1: All Islands must be connected  
2.2: Cycles are possible

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

## Rule 3

3.1 Bridges don't cross other bridges  
3.2 Brdiges don't cross islands

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
