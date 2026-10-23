//! Native contracts for allocation, slice reads, push/update and parent traversal.
use super::lemma_kernel;
use std::{eprintln, vec};
use std::vec::Vec;
use std::collections::VecDeque;

fn reference(n: usize, k: usize, g: usize, after: &[usize], commits: &[bool]) -> (Vec<bool>, Vec<Option<(usize, usize)>>, Vec<usize>) {
    let mut seen = vec![false; n];
    let mut parents = vec![None; n];
    let mut depth = vec![0; n];
    let mut todo = VecDeque::from([g]);
    seen[g] = true;
    while let Some(s) = todo.pop_front() {
        for i in 0..k {
            let next = if commits[s*k+i] { after[s*k+i] } else { s };
            if !seen[next] {
                seen[next] = true;
                parents[next] = Some((s,i));
                depth[next] = depth[s]+1;
                todo.push_back(next);
            }
        }
    }
    (seen,parents,depth)
}

#[test]
fn reachable_matches_independent_fifo_on_all_small_tables() {
    let mut cases = 0;
    for n in 1usize..=3 {
        for k in 1usize..=2 {
            let edges = n*k;
            for number in 0..n.pow(edges as u32) {
                let mut digits=number;
                let after: Vec<usize>=(0..edges).map(|_| {let value=digits%n;digits/=n;value}).collect();
                for bits in 0usize..1<<edges {
                    let commits:Vec<bool>=(0..edges).map(|j| bits&(1<<j)!=0).collect();
                    for g in 0..n {
                        let actual=lemma_kernel::reachable(n,k,g,&after,&commits);
                        let expected=reference(n,k,g,&after,&commits);
                        assert_eq!(actual.seen,expected.0);
                        assert_eq!(actual.parent,expected.1);
                        assert_eq!(actual.depth,expected.2);
                        cases+=1;
                    }
                }
            }
        }
    }
    eprintln!("reachability tables: {cases}");
}

#[test]
fn vector_contracts_have_native_witnesses_and_exclusive_results() {
    let after=vec![1,2,2];
    let commits=vec![true,true,false];
    let a=lemma_kernel::reachable(3,1,0,&after,&commits);
    let mut b=lemma_kernel::reachable(3,1,0,&after,&commits);
    b.seen[0]=false;
    b.parent[1]=None;
    b.depth[1]=999;
    assert!(a.seen[0]);
    assert_eq!(a.parent[1],Some((0,0)));
    assert_eq!(a.depth,vec![0,1,2]);
    assert_eq!(after,vec![1,2,2]);
    assert_eq!(commits,vec![true,true,false]);
    let mut values=vec![7usize;3];
    values[1]=9;
    values.push(11);
    assert_eq!(&values[..],&[7,9,7,11]);
    let mut inputs=vec![1,2,3];
    inputs.reverse();
    assert_eq!(inputs,vec![3,2,1]);
}
