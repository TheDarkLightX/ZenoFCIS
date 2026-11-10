//! Pinned dual-result udivrem versus baseline, with real live-across-div call outputs.
//! Host-specific exploratory Wasmtime in-process timing; not a compiler proof.
use std::{hint::black_box,time::Instant};
use wasmtime::{Engine,Instance,Module,Store,TypedFunc};
type E=Box<dyn std::error::Error>;
const SCHEMA:&str="zenofcis/udivrem-live-inprocess/1";
const PAIRS:[(i64,i64);16]=[(0,1),(7,3),(i64::MIN,-1),(i64::MAX,1),
    (-1,-1),(i64::MAX,i64::MIN),(i64::MIN,i64::MAX),(123456789,-7),
    (0,-1),(-1,1),(i64::MIN,3),(i64::MAX,7),
    (7,2),(i64::MIN,2),(123456789,98765),(-987654321,31337)];
fn oracle(x:i64,y:i64,n:usize,mode:&str)->i64 {
    let (a,b)=(x as u64,y as u64);
    let mut v=if mode=="quotient"{a/b}else{a/b^(a%b).rotate_left(17)};
    for i in 0..n {v^=(a^(3+2*i as u64))^17;}
    v as i64
}
struct Guest{store:Store<()>,f:TypedFunc<(i64,i64),i64>}
impl Guest {
    fn load(e:&Engine,p:&str)->Result<Self,E>{
        // SAFETY: CI independently verifies pinned producer and exact AOT hashes first.
        let m=unsafe{Module::deserialize_file(e,p)}?;
        let mut store=Store::new(e,());
        let instance=Instance::new(&mut store,&m,&[])?;
        let f=instance.get_typed_func::<(i64,i64),i64>(&mut store,"work")?;
        Ok(Self{store,f})
    }
    fn check(&mut self,n:usize,mode:&str)->Result<(),E>{
        for &(x,y) in &PAIRS {
            let got=self.f.call(&mut self.store,(x,y))?;
            if got!=oracle(x,y,n,mode){return Err(format!("wrong value for {x},{y}").into());}
        }
        if self.f.call(&mut self.store,(1,0)).is_ok(){return Err("missing zero-divisor trap".into());}
        Ok(())
    }
    fn batch(&mut self,count:usize,want:u64)->Result<(),E>{
        let mut sum=0u64;
        for i in 0..count {
            let (x,y)=black_box(PAIRS[i%PAIRS.len()]);
            sum=sum.wrapping_add(black_box(self.f.call(&mut self.store,(x,y))? as u64));
        }
        if sum!=want{return Err("wrong batch checksum".into());}
        Ok(())
    }
}
fn run()->Result<(),E>{
    let a:Vec<_>=std::env::args().collect();
    if a.len()!=9{return Err("usage: runner CASE N MODE BASE.cwasm PATCH.cwasm CALLS BLOCKS SEED".into());}
    let n:usize=a[2].parse()?;
    let mode=a[3].as_str();
    let calls:usize=a[6].parse()?;
    let blocks:usize=a[7].parse()?;
    let seed:usize=a[8].parse()?;
    let admitted=match a[1].as_str(){
        "live_0"=>(0,"forward"),"live_4"=>(4,"forward"),
        "live_12"=>(12,"forward"),"live_20"=>(20,"forward"),
        "live_12_reverse"=>(12,"reverse"),
        "live_12_quotient_only"=>(12,"quotient"),_=>return Err("unknown case".into())
    };
    if (n,mode)!=admitted || !(10_000..=1_000_000).contains(&calls)
       || !(4..=32).contains(&blocks)||blocks%2!=0||seed!=5623{
       return Err("bad contract or timing budget".into());
    }
    let e=Engine::default();
    let mut b=Guest::load(&e,&a[4])?;
    let mut p=Guest::load(&e,&a[5])?;
    b.check(n,mode)?;p.check(n,mode)?;
    let mut checksum=0u64;
    for i in 0..calls{
        let (x,y)=PAIRS[i%PAIRS.len()];
        checksum=checksum.wrapping_add(oracle(x,y,n,mode) as u64);
    }
    for _ in 0..2{b.batch(calls,checksum)?;p.batch(calls,checksum)?;}
    println!("META,{SCHEMA},{},{n},{mode},{calls},{blocks},{seed}",a[1]);
    println!("CHECK,baseline,16");
    println!("CHECK,patched,16");
    for block in 0..blocks{
        let order=if block%2==0{["baseline","patched","patched","baseline"]}
                  else{["patched","baseline","baseline","patched"]};
        for (position,name) in order.iter().enumerate(){
            let guest=if *name=="baseline"{&mut b}else{&mut p};
            let t=Instant::now();guest.batch(calls,checksum)?;
            let ns=t.elapsed().as_nanos();
            if ns==0{return Err("zero-time sample".into());}
            println!("SAMPLE,{block},{position},{name},{ns},{checksum}");
        }
    }
    Ok(())
}
fn main(){if let Err(e)=run(){eprintln!("call-live perf error: {e}");std::process::exit(1);}}
