const field=(path,index,source,domain,initial,extra={})=>({path,index,source,domain,initial,options:[],type_name:path,...extra});
const requirements=(primary,context,groups=[],grouped=false)=>({
  format:'zenoui/requirements/2',primaryFields:primary,detailFields:[],fixedFields:[],contextFields:context,
  groups,groupCommands:grouped,collapseExpert:true,compact:true,maxVisibleForms:16,
  maxExpandedExpertPanels:0,maxColumns:2,projections:[],quoteOverrides:{},
});

export function editFixture(){
  return {raw:{fields:[
    field('pre.100.110',0,'state',{kind:'int',min:'0',max:'9'},'2'),
    field('command.101.120',1,'command',{kind:'sum',type_id:120,variants:[120]},'120',{options:[{value:'120'}]}),
    field('command.101.121',2,'command',{kind:'int',min:'0',max:'9'},'2'),
    field('context.102.130',3,'context',{kind:'bool'},'0'),
    field('context.102.131',4,'context',{kind:'int',min:'0',max:'5'},'0',{clock:true}),
  ],reasons:[],enablement:{table:[false,false,false,true]},domain:'1200'},
  meta:{format:'zenoui/metadata/2',title:'Adjust stock',action:'command.101.120',
    fields:{'pre.100.110':{label:'Current stock',unit:'items'},
      'command.101.120':{label:'Action',unit:'unitless'},
      'command.101.121':{label:'Requested stock',unit:'items',role:'requested-value',stateField:'pre.100.110'},
      'context.102.130':{label:'Manager',unit:'unitless'},
      'context.102.131':{label:'Clock',unit:'seconds'}},
    variants:{'120':'Set stock'},commands:{'120':{label:'Set stock',consequential:true,uiRole:'update'}},reasons:{},
    requirements:requirements(['command.101.121'],['context.102.130','context.102.131'])}};
}

export function approvalFixture(){
  return {raw:{fields:[
    field('pre.200.210',0,'state',{kind:'int',min:'0',max:'2'},'0'),
    field('command.201.220',1,'command',{kind:'sum',type_id:220,variants:[220,221]},'220',{options:[{value:'220'},{value:'221'}]}),
    field('context.202.230',2,'context',{kind:'bool'},'0'),
  ],reasons:[],enablement:{table:[false,false,false,true]},domain:'12'},
  meta:{format:'zenoui/metadata/2',title:'Review one request',action:'command.201.220',
    fields:{'pre.200.210':{label:'Request status',unit:'unitless',role:'approval-status',values:{'0':'Pending','1':'Approved','2':'Declined'}},
      'command.201.220':{label:'Decision',unit:'unitless'},'context.202.230':{label:'Reviewer',unit:'unitless'}},
    variants:{'220':'Approve','221':'Decline'},
    commands:{'220':{label:'Approve',consequential:true,uiRole:'approve'},'221':{label:'Decline',consequential:true,uiRole:'reject'}},
    reasons:{},requirements:requirements([],['context.202.230'],[{id:'decision',commands:['220','221']}],true)}};
}
