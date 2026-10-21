#!/usr/bin/env python3
"""Build isolated monetary faults, then require named semantic failures."""
from pathlib import Path
import os,json,shutil,subprocess,difflib,hashlib,time,argparse
ROOT=Path(__file__).resolve().parents[3]; APP=ROOT/'core-apps/zusd-v1'
parser=argparse.ArgumentParser();parser.add_argument('--out',type=Path,required=True);parser.add_argument('--only',nargs='*');args=parser.parse_args();OUT=args.out.resolve();OUT.mkdir(parents=True,exist_ok=True)
TRANS='src/application/transition.rs'; PUB='src/application/publication.rs'
replay=('round6_replay','v1_departures_retain_exact_state_threaded_inputs_and_decisions')
journey=('step4','multi_user_pending_commit_liquidation_redemption_claims_halt_recovery')
mint='''if let Command::Fund { recipient, quantity: Quantity::Zusd(n), .. } = &r.command {
            let minted=ledger::compose(&pre.ledger,&[Movement {from:None,to:Some(*recipient),quantity:Quantity::Zusd(*n)}],(add(pre.ledger.active_debt,*n,w)?,pre.ledger.default_debt),p.config.bounds.balances,limits).into_parts().0?;
            post.ledger=minted.post().clone(); post.book.system.debt=add(post.book.system.debt,*n,w)?;
        } else { apply(p, pre, r, &mut post, &mut legs, &mut trace, w)?; }'''
faults=[
('price-before-commit',replay,[('src/liquidation.rs','partition(v, running, prices.committed, available, mode, w)?','partition(v, running, prices.pending.unwrap_or(prices.committed), available, mode, w)?')]),
('upward-report-blocked',replay,[(TRANS,'OracleAction::ImportPending => {','OracleAction::ImportPending => {\n            if old.and_then(|v|v.committed).is_some_and(|c|o.view.pending.is_some_and(|n|n.value_e8>c.value_e8)){return Err(Failure::Canonical);} ')]),
('commit-below-mcr-stuck',replay,[(TRANS,'let committed = o.view.committed.ok_or(Failure::Canonical)?;','let committed = o.view.committed.ok_or(Failure::Canonical)?;\n            for v in &s.book.vaults { if v.status==vaults::Status::Active && vaults::ratio(v.collateral,v.debt,committed.value_e8,w)? < vaults::MCR {return Err(Failure::Reject(1043));} }')]),
('withdraw-before-drop',('native_relations','stability_both_price_principal_guard_and_history_integrity'),[('src/stability.rs','[Some(prices.committed), prices.pending]','[Some(prices.committed), None]')]),
('wrong-cap-branch',('native_relations','d4_capped_surplus_is_not_retained_in_recovery_backing'),[('src/liquidation.rs','if mode && icr >= MCR {','if mode && icr < MCR {')]),
('gas-payment-twice',journey,[(TRANS,'Quantity::Zusd(part.reserve));','Quantity::Zusd(part.reserve * 2));')]),
('reserve-burn-twice',('step4','approved_change_preserves_live_reserves_and_closes_each_once'),[(TRANS,'Quantity::Zusd(o.reserve_burned));','Quantity::Zusd(o.reserve_burned * 2));')]),
('reserve-debt-skipped',('economic_reference','independent_owner_complete_outcome_reason_and_frame_rows'),[('src/vaults.rs','let debt = add(net, params.reserve, w)?;','let debt = net;')]),
('assets-swapped',('round6_controls','ledger_debit_matches_the_exact_asset_transfer'),[('src/ledger.rs','Quantity::Agrs(_) => Quantity::Agrs(value),','Quantity::Agrs(_) => Quantity::Zdex(value),')]),
('fee-rounding-drift',('native_relations','fees_match_independent_integer_reference_and_unbound_clock'),[('src/fees.rs','pub const FLOOR: u128 = 5_000_000_000_000_000;','pub const FLOOR: u128 = 5_000_000_000_000_001;')]),
('reward-claimed-twice',('round6_controls','staking_reward_is_consumed_once_with_other_claims_outstanding'),[('src/staking.rs','snapshot_z: s.f_z,','snapshot_z: old.map_or(s.f_z, |p|p.snapshot_z),'),('src/staking.rs','snapshot_a: s.f_a,','snapshot_a: old.map_or(s.f_a, |p|p.snapshot_a),')]),
('ledger-debit-missing',('round6_controls','ledger_debit_matches_the_exact_asset_transfer'),[('src/accounting.rs','old.checked_sub(amount)','Some(old)')]),
('unrestricted-mint',('step4','funding_is_authenticated_asset_specific_and_single_use'),[(TRANS,'apply(p, pre, r, &mut post, &mut legs, &mut trace, w)?;',mint),(TRANS,'        validate(p, &post, w)?;','        if !matches!(r.command, Command::Fund {quantity:Quantity::Zusd(_),..}) {validate(p, &post, w)?;}')]),
('mismatched-receipt',('step4','stale_borrowed_replayed_receipts_and_backend_failures_refuse'),[(TRANS,'.map_err(|e| match e {','.or(Ok::<(), VerificationFailure>(()))\n            .map_err(|e| match e {')]),
('partial-publication',('step4','atomic_publication_survives_all_six_crash_points'),[(PUB,'let temp = self.directory.join("pending");','let temp = self.path();')]),
('payout-recorded-twice',('step4','cas_conflict_retry_and_cold_replay_never_duplicate_economics'),[(PUB,'current.rows.push(checked.bytes);','current.rows.push(checked.bytes.clone());\n        current.rows.push(checked.bytes);')]),
]
if args.only:faults=[f for f in faults if f[0] in args.only]
rows=[]; prefix=['nice','-n','19','ionice','-c3']; env=dict(os.environ)
env.pop('RUSTFLAGS',None);env.pop('RUSTC_BOOTSTRAP',None)
env['CARGO_BUILD_JOBS']='3'
assert Path(env['CARGO_TARGET_DIR']).is_absolute()
assert Path(env['TMPDIR']).is_absolute()
def run(label,app,test,build=False):
    command=prefix+['cargo','+1.97.1','test','--locked','--offline','--manifest-path',str(app/'Cargo.toml'),'--features','std','--test',test[0]]
    command+=['--no-run'] if build else [test[1],'--','--exact','--nocapture']
    start=time.monotonic();r=subprocess.run(command,cwd=ROOT,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
    log=OUT/(label+'.log');log.write_text(r.stdout)
    rows.append({'label':label,'command':command,'exit':r.returncode,'seconds':round(time.monotonic()-start,3),'log':log.name,'sha256':hashlib.sha256(log.read_bytes()).hexdigest()})
    (OUT/'runs.json').write_text(json.dumps(rows,indent=2)+'\n');return r
for test in sorted(set(test for _,test,_ in faults)):
    r=run('baseline-'+test[1],APP,test);assert r.returncode==0 and '1 passed' in r.stdout,r.stdout[-2500:]
results=[]
for name,test,edits in faults:
    location=OUT/'copies'/name
    if location.exists():raise RuntimeError('refuse overwrite '+str(location))
    location.mkdir(parents=True);(location/'crates').symlink_to(ROOT/'crates',target_is_directory=True);(location/'Cargo.toml').symlink_to(ROOT/'Cargo.toml')
    candidate=location/'core-apps/zusd-v1';candidate.parent.mkdir();shutil.copytree(APP,candidate,ignore=shutil.ignore_patterns('target','__pycache__'))
    package='zenofcis-zusd-v1-control-'+name
    for path in ['Cargo.toml','Cargo.lock']:
        f=candidate/path;s=f.read_text().replace('name = "zenofcis-zusd-v1"','name = "'+package+'"')
        if path=='Cargo.toml':s+='\n[lib]\nname = "zenofcis_zusd_v1"\n'
        f.write_text(s)
    for path,before,after in edits:
        f=candidate/path;s=f.read_text();assert s.count(before)==1,(name,path,s.count(before));f.write_text(s.replace(before,after))
    patch=''.join(''.join(difflib.unified_diff((APP/path).read_text().splitlines(True),(candidate/path).read_text().splitlines(True),fromfile='a/'+path,tofile='b/'+path)) for path in sorted(set(p for p,_,_ in edits)))
    (OUT/(name+'.patch')).write_text(patch)
    built=run(name+'-build',candidate,test,True)
    if built.returncode:
        print('BUILD FAILURE',name,built.stdout[-1600:],flush=True);results.append({'name':name,'build':built.returncode,'killed':False});continue
    bad=run(name+'-negative',candidate,test)
    killed=(bad.returncode==101 and ('test '+test[1]+' ... FAILED' in bad.stdout) and '1 failed' in bad.stdout
        and not any(message in bad.stdout for message in ('attempt to multiply with overflow','attempt to add with overflow','attempt to subtract with overflow','memory allocation of','internal compiler error')))
    results.append({'name':name,'build':built.returncode,'negative':bad.returncode,'named_test':test[1],'killed':killed,'patch_sha256':hashlib.sha256(patch.encode()).hexdigest()})
    (OUT/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
    print(name,'KILLED' if killed else 'SURVIVED',flush=True)
print(json.dumps(results,indent=2));assert all(r['killed'] for r in results)
