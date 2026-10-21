use super::*;
use vstd::prelude::*;
verus! {
pub(in super::super) open spec fn ascii(bytes:Seq<u8>)->bool{forall|i:int|0<=i<bytes.len()==>bytes[i]<128}
pub(in super::super) open spec fn atom(domain:Domain,a:Atom)->bool{decision::spec::admitted(domain,a)&&match a{Atom::Text(bytes)=>ascii(bytes@),_=>true}}
pub(in super::super) open spec fn payload_entry(f:Seq<Field>,s:Seq<TypedField>,i:int)->bool{f[i].id==s[i].field&&atom(s[i].domain,f[i].value)}
pub(in super::super) open spec fn payload(f:Seq<Field>,s:Seq<TypedField>)->bool{f.len()==s.len()&&(forall|i:int|0<=i<f.len()==>payload_entry(f,s,i))}
pub(in super::super) open spec fn delivery(d:&Descriptor,p:decision::spec::DeliveryView)->bool{match admission::spec::channel(d,p.1){None=>false,Some(c)=>atom(c.destination,p.2)&&atom(c.idempotency,p.4)&&payload(p.3,c.payload@)}}
pub(in super::super) open spec fn deliveries(d:&Descriptor,p:Seq<decision::spec::DeliveryView>)->bool{forall|i:int|0<=i<p.len()==>delivery(d,p[i])}
pub(in super::super) open spec fn candidate(d:&Descriptor,c:decision::spec::CandidateView)->bool{deliveries(d,c.5)&&deliveries(d,c.6)}
pub(in super::super) open spec fn expression(domain:Domain,e:Expr)->bool{match e{Expr::Constant(a)=>atom(domain,a),_=>true}}
pub(in super::super) open spec fn law_literal(node:laws::Op)->bool{match node{laws::Op::Literal(laws::Atom::Text(bytes))=>ascii(bytes@),_=>true}}
pub(in super::super) open spec fn law_nodes(nodes:Seq<laws::Op>)->bool{forall|i:int|0<=i<nodes.len()==>law_literal(nodes[i])}
pub(in super::super) open spec fn law_literals(definitions:Seq<laws::Law>)->bool{forall|i:int|0<=i<definitions.len()==>law_nodes(definitions[i].program.nodes@)}
}
