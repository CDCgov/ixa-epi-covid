# Grammar for activity spaces and modification

## Definitions
1. `action`: something a person is doing for a percentage of their time (e.g. `home`, `work`, `education`, `religion`, `entertainment`, etc.).
2. `itinerary`: the proportion of time an individual spends in each locations in their activity space.
3. `location`: physical space
4. `location category`: the type of location, which possesses particular traits
5. `activity space`: the set of all location that an individual would normally spend time in
6. `action modifier`: a change to the allotment of how an individual spends their time, such as not going to work or school on the weekends or avoiding going out during shelter in place
7. `space usage modifier`: a change to where an individual conducts an action, such as remote schooling or avoiding bars and restaurants.
8. `itinerary modifier`: a paired set of an action modifier and a space usage modifier that change how an individual spends their time across locations.

## Definition
Imagine we have a person who spends their time doing $m$ actions across $n$ locations. Call the proportion of time that a person spends in each location of their activity space an "itinerary". An itinerary for person $i$ is the matrix $A_i$, which is an $n \times 1$ matrix. The proportion of time that a person spends doing each action is the matrix $B_i$, which is a $m\times 1$ matrix. In order to arrive at $A_i$ from $B_i$, we require information on the proportion of an action that a person does across locations in their activity space. We call this the space assignment matrix, $M_i$, which is an $m \times n$ matrix. We can therefore say that

$$ A = M B, $$

removing subscripts for clarity.

Both the space assignment matrix $M$ and the action vector can be modified. Space usage modifiers are $n \times n$ transition matrices, the columns of which each sum to one. Action modifiers are $m \times m$ transition matrices, the columns of which each sum to one. If we define matrix $Q$ to be the product of all active space usage modifiers and deine matrix $R$ to be the product of all active action modifiers, then the realized itinerary $\hat{A}$ of a person becomes

$$ \hat{A} = QMRB $$

Setting $Q=I$ and $R=I$ in the above equation, where $I$ is the identity matrix, recovers the original default itinerary vector $A$.

### Arbitrary example
A person has action vector

$$ B = \begin{array}{|c|} p_i \\ p_j \\ p_k \\ \end{array} $$

for $m=3$ actions: $i, j$, and $k$. Their space assignment matrix $M$ for four locations ($w, x, y,$ and $z$) is

$$
M = \begin{array}{|ccc|}
q_{i,w}=1 & 0 & 0 \\
0 & q_{j,x} & 0 \\
0 & 0 & q_{k,y} \\
0 & q_{j,z} & q_{k,z} \\
\end{array}
$$

where $q_{a,s}$ is the proportion of action $a$ spent in location $s$.

Multiplying the action vector by the space assignment matrix, we get the itinerary

$$ A = \begin{array}{|c|}
p_iq_{i,w} \\
p_jq_{j,x} \\
p_kq_{k,y} \\
p_jq_{j,z} + p_kq_{k,z} \\
\end{array} $$

### Example 1: isolation space usage modifier + weekend action modifier
A person has $n=4$ spaces that they could use: (Home, Work, School, Community). They do $m=4$ actions: (Home, Work, School, Community). We define

$$ B = \begin{array}{|c|}
0.3 \\\ 0.0 \\\ 0.5 \\\ 0.2
\end{array}$$

$$ M = \begin{array}{|cccc|}
1.0 & 0 & 0 & 0 \\
0 & 1.0 & 0 & 0 \\
0 & 0 & 1.0 & 0\\
0 & 0 & 0 & 1.0 \\
\end{array}$$

$$ A = MB = IB = B  $$

We apply a policy "isolation", comprised of space usage modifier $Q_{\text{isolate}}$, which moves school and work actions to take place at home, and $R_{\text{isolate}}$, which reduces time spent doing Community in half.

$$ Q_{\text{isolate}} = \begin{array}{|cccc|}
1.0 & 1.0 & 1.0 & 0 \\
0 & 0 & 0 & 0 \\
0 & 0 & 0 & 0\\
0 & 0 & 0 & 1.0 \\
\end{array}$$

$$ R_{\text{isolate}} = \begin{array}{|cccc|}
1.0 & 0 & 0 & 0.5 \\
0 & 1.0 & 0 & 0 \\
0 & 0 & 1.0 & 0\\
0 & 0 & 0 & 0.5 \\
\end{array}$$

We also have a weekend action modifier, $R_{\text{weekend}}$, which re-allocates time spent doing work and school to be split evenly across home and community

$$ R_{\text{weekend}} = \begin{array}{|cccc|}
1.0 & 0.5 & 0.5 & 0 \\
0 & 0 & 0 & 0 \\
0 & 0 & 0 & 0\\
0 & 0.5 & 0.5 & 1.0 \\
\end{array}$$

We assert that $R_{\text{weekend}}$ is basal, meaning that "isolate" action changes modify "weekend" actions and that "weekend" actions do not modify "isolate" actions, such that the observed space usage modifier $R_{\text{obs}}=R_{\text{isolate}}R_{\text{weekend}}$. We can now calculate the realized itinerary $\hat{A}$ as

$$\hat{A}=Q_{\text{isolate}}MR_{\text{obs}}B=\begin{array}{|cccc|}
1.0 & 0.75 & 0.75 & 0.5 \\
0 & 0 & 0 & 0 \\
0 & 0 & 0 & 0\\
0 & 0.25 & 0.25 & 0.5 \\
\end{array}\times\begin{array}{|c|}
0.3 \\\ 0.0 \\\ 0.5 \\\ 0.2
\end{array}=\begin{array}{|c|}
0.775 \\\ 0.0 \\\ 0.0 \\\ 0.225 \\\
\end{array}$$
