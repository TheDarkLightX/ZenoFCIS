{-# LANGUAGE BangPatterns #-}
module Zal.Logic.Compare
  ( Relation(..), Partition(..), Comparison(..), compareAt, violates ) where

import Control.Monad (foldM, unless)
import qualified Data.Map.Strict as M
import Zal.Logic.Syntax
import Zal.Logic.Finite

data Relation = Equivalence | Implication deriving (Eq,Show)
data Partition = Partition
  { bothTrue :: !Integer, leftOnly :: !Integer
  , rightOnly :: !Integer, bothFalse :: !Integer
  } deriving (Eq,Show)
data Comparison = Comparison
  { partition :: !Partition, enumerated :: !Integer, admitted :: !Integer
  , workBound :: !Integer, witness :: !(Maybe (Structure,Bool,Bool))
  } deriving (Eq,Show)

violates :: Relation -> Bool -> Bool -> Bool
violates Equivalence a b = a/=b
violates Implication a b = a && not b

compareAt :: Limits -> Relation -> Checked -> Checked -> Checked -> Either Failure Comparison
compareAt lim relation assumptions left right = do
  let qa=checkedQuery assumptions
      ql=checkedQuery left
      qr=checkedQuery right
      n=domainSize qa
      same q=domainSize q==n && signature q==signature qa && profile q==profile qa
      unit q=all (==Weight 1 1) (M.elems (weights q))
      atoms=sum [n^(case a of Unary->(1::Integer); Binary->2) | a<-M.elems (signature qa)]
      visits=sum [evaluationWork n (sentence q) | q<-[qa,ql,qr]]
  unless (all same [ql,qr]) (Left (Invalid "comparison domain/signature/profile mismatch"))
  unless (all unit [qa,ql,qr]) (Left (Invalid "logical comparison requires unit weights"))
  total<-boundedPower (maxCandidates lim) atoms
  -- Include witness replay and structure construction in the conservative bound.
  let bound=total*(visits+atoms+1)+visits
  unless (bound<=maxWork lim) (Left (ResourceExhausted "comparison work bound" bound (maxWork lim)))
  models<-enumerateStructures lim assumptions
  (!hist,!first)<-foldM step (Partition 0 0 0 0,Nothing) models
  let count=bothTrue hist+leftOnly hist+rightOnly hist+bothFalse hist
  case first of
    Nothing -> unless (leftOnly hist==0 && (relation==Implication || rightOnly hist==0))
                 (Left (InternalDisagreement "missing comparison witness"))
    Just (m,a,b) -> do
      valid<-evaluate assumptions m
      a'<-evaluate left m
      b'<-evaluate right m
      unless (valid && a==a' && b==b' && violates relation a' b')
        (Left (InternalDisagreement "counterexample replay disagrees"))
  Right (Comparison hist total count bound first)
  where
    step (!h,!first) m = do
      allowed<-evaluate assumptions m
      if not allowed then Right (h,first) else do
        a<-evaluate left m
        b<-evaluate right m
        let !updated=case (a,b) of
              (True,True)->h {bothTrue=bothTrue h+1}
              (True,False)->h {leftOnly=leftOnly h+1}
              (False,True)->h {rightOnly=rightOnly h+1}
              (False,False)->h {bothFalse=bothFalse h+1}
            !found=case first of
              Nothing | violates relation a b -> Just (m,a,b)
              _ -> first
        Right (updated,found)
