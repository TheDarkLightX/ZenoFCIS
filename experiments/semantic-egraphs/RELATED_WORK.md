# Primary sources

- Ross Tate, Michael Stepp, Zachary Tatlock, and Sorin Lerner.
  *Equality Saturation: A New Approach to Optimization*. POPL 2009.
  [Author page](https://www.cs.cornell.edu/~lerner/papers/popl09.html).
  Equality saturation and translation validation precede this experiment.
- Max Willsey, Chandrakana Nandi, Yisu Remy Wang, Oliver Flatt,
  Zachary Tatlock, and Pavel Panchekha. *egg: Fast and Extensible
  Equality Saturation*. Proceedings of the ACM on Programming Languages 5,
  POPL, Article 23, 2021. DOI 10.1145/3434304.
  [Paper](https://arxiv.org/abs/2004.03082).
  This experiment uses egg's existing e-class analysis and extraction facilities.
- Randal E. Bryant. *Graph-Based Algorithms for Boolean Function
  Manipulation*. IEEE Transactions on Computers C-35(8), 677--691,
  August 1986. DOI 10.1109/TC.1986.1676819.
  [Author's bibliography](https://www.cs.cmu.edu/~bryant/pubs.html).
  Decision diagrams are a relevant alternative; no BDD benchmark is performed.
- Alan Mishchenko, Satrajit Chatterjee, and Robert Brayton.
  *DAG-Aware AIG Rewriting: A Fresh Look at Combinational Logic Synthesis*.
  DAC 2006. DOI 10.1145/1146909.1147048.
  [Author-hosted paper](https://people.eecs.berkeley.edu/~alanmi/publications/2006/dac06_rwr.pdf).
  Shared circuit optimization is established prior work; no ABC comparison is performed.
- Jiaqi Yin, Zhan Song, Chen Chen, Qihao Hu, and Cunxi Yu.
  *BoolE: Exact Symbolic Reasoning via Boolean Equality Saturation*.
  DAC 2025; arXiv:2504.05577.
  [Paper](https://arxiv.org/abs/2504.05577).
  Boolean equality saturation is established prior work.
- Sijie Kong, Jingtao Xia, Daniel Ruelas-Petrisko, Zachary D. Sisco,
  Jonathan Balkind, and Gus Henry Smith. *Improving Equality Saturation
  for EDA via Semantic E-Graphs*. Proceedings of the ACM on Programming
  Languages 10, PLDI, Article 221, June 2026. DOI 10.1145/3808299.
  [Author-hosted paper](https://zsisco.net/papers/nextmap-pldi26.pdf).
  Semantic identities and semantic e-graphs are established prior work. This
  experiment uses full Boolean truth tables as analysis data in ordinary egg;
  it does not implement Nextmap's semantic-identifier engine or compare with it.
- Ohad Asor. *Guarded Successor: A Novel Temporal Logic*.
  arXiv:2407.06214v1, 4 July 2024.
  [Paper](https://arxiv.org/html/2407.06214v1).
  Sections 2.3--2.4 describe a finite quotient of formulas under a fixed finite
  free-variable and constant scope, including quantified formulas via quantifier
  elimination. The present propositional Boolean experiment does not implement
  this quantified or temporal theory and does not establish a Tau integration.

The intended contribution is a bounded, independently checked ZenoFCIS case
study and its reproducible evidence, rather than a new Boolean-equivalence
algorithm or a claim of superiority to mature circuit synthesis tools.
