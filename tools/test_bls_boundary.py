"""Independent regression for transitive semantic/dev dependency exclusion."""
import tempfile,unittest
from pathlib import Path
from check_assurance import check_shell_crypto_boundary
class Boundary(unittest.TestCase):
    def check_graph(self, dependencies):
        with tempfile.TemporaryDirectory() as temp:
            paths=[]
            for i,(name,deps,kind) in enumerate(dependencies):
                p=Path(temp)/str(i);p.write_text('[package]\nname="'+name+'"\n['+kind+']\n'+''.join(n+'='+('\"=1.0.0\"' if not n.startswith('alias') else '{ package=\"blst\", version=\"=0.3.17\" }')+'\n' for n in deps));paths.append(p)
            return check_shell_crypto_boundary(paths)
    def test_inward_adapter_is_allowed(self):
        self.assertEqual(self.check_graph([('zeno-fcis-bls',['blst','zenofcis-zusd-v1'],'dependencies'),('zenofcis-zusd-v1',['zeno-fcis-value'],'dependencies')]),[])
    def test_transitive_and_test_edges_refuse(self):
        for kind in ['dependencies','dev-dependencies','build-dependencies']:
            with self.subTest(kind=kind):
                self.assertTrue(self.check_graph([('zeno-fcis-core',['helper'],kind),('helper',['zeno-fcis-bls'],'dependencies')]))
    def test_renamed_external_dependency_refuses(self):
        self.assertTrue(self.check_graph([('zeno-fcis-core',['alias'],'dependencies')]))
    def test_standalone_semantic_crate_refuses_raw_blst(self):
        self.assertTrue(self.check_graph([('zenofcis-zusd-v1',['blst'],'dev-dependencies')]))
if __name__=='__main__':unittest.main()
