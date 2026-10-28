module Zal.Logic.Expansion (expansionsAt) where
import Control.Monad (replicateM)
import qualified Data.Map.Strict as M
import Zal.Logic.Syntax
import Zal.Logic.Normalize
import Zal.Logic.Finite

-- Bounded pointwise audit. Enumerates auxiliary assignments for ONE supplied
-- original structure. It does not use population recurrence or type partitions.
expansionsAt :: Limits -> Checked -> Structure -> Either Failure (Integer,Bool)
expansionsAt lim checked model = do
  original <- evaluate checked model
  let norm=normalize checked
      dom=[0..domainSize (checkedQuery checked)-1]
      rkeys=[(rowId r,a) | r<-rows norm,rowObligation r==Annotation,a<-dom]
      gkeys=[globalId g | g<-globals norm,globalObligation g==Annotation]
      size=length rkeys+length gkeys
  _ <- boundedPower (maxCandidates lim) (toInteger size)
  let satisfies bits =
        let (rb,gb)=splitAt (length rkeys) bits
            rm=M.fromList (zip rkeys rb); gm=M.fromList (zip gkeys gb)
            ev env=qEval (\p vs->atomAt model p (map env vs)) (\a b->env a==env b)
                         (\i v->M.findWithDefault False (i,env v) rm) (\i->M.findWithDefault False i gm)
            oneRow r a =
              let countN=toInteger (length [b | b<-dom,ev (\v->if v==owner r then a else b) (rowBody r)])
                  truth=testCount (rowTest r) countN
              in case rowObligation r of Required->truth; Annotation->M.lookup (rowId r,a) rm==Just truth
            oneGlobal g =
              let countN=toInteger (length [a | a<-dom,ev (const a) (globalBody g)])
                  truth=testCount (globalTest g) countN
              in case globalObligation g of Required->truth; Annotation->M.lookup (globalId g) gm==Just truth
        in ev (const 0) (root norm)
          && all (\(a,b,u)->ev (\v->if v==X then a else b) u) [(a,b,u)|a<-dom,b<-dom,u<-universal norm]
          && all (\(r,a)->oneRow r a) [(r,a)|r<-rows norm,a<-dom]
          && all oneGlobal (globals norm)
      number=toInteger (length (filter satisfies (replicateM size [False,True])))
  Right (number,original)
