module Zal.Logic.Normalize
  ( QFormula(..), Obligation(..), Row(..), Global(..), Normal(..), Definition(..)
  , normalize, qEval, qFromFormula, normalIdentities
  ) where

import qualified Data.Map.Strict as M
import qualified Data.Set as S
import Zal.Logic.Syntax

data QFormula
  = QTruth !Bool | QAtom !String ![Var] | QEqual !Var !Var
  | QNot !QFormula | QAnd !QFormula !QFormula | QOr !QFormula !QFormula
  | QImplies !QFormula !QFormula | QIff !QFormula !QFormula
  | QRow !Integer !Var | QNull !Integer
  deriving (Eq, Ord, Show)
data Obligation = Required | Annotation deriving (Eq, Ord, Show)
data Row = Row
  { rowId :: !Integer, owner :: !Var, counted :: !Var, rowTest :: !Test
  , rowBody :: !QFormula, rowObligation :: !Obligation
  } deriving (Eq, Show)
data Global = Global
  { globalId :: !Integer, globalVar :: !Var, globalTest :: !Test
  , globalBody :: !QFormula, globalObligation :: !Obligation
  } deriving (Eq, Show)
data Definition = Definition
  { definitionId :: !Integer, definitionSource :: !Formula
  , definitionRelation :: !String, definitionRole :: !String
  } deriving (Eq, Show)
data Normal = Normal
  { source :: !Checked, universal :: ![QFormula], rows :: ![Row]
  , globals :: ![Global], root :: !QFormula, definitions :: ![Definition]
  } deriving (Eq, Show)

data Build = Build !Integer ![Row] ![Global] ![QFormula] ![Definition]

qFromFormula :: Formula -> Maybe QFormula
qFromFormula f = case f of
  Truth b -> Just (QTruth b)
  Atom p vs -> Just (QAtom p vs)
  Equal a b -> Just (QEqual a b)
  Not a -> QNot <$> qFromFormula a
  And a b -> QAnd <$> qFromFormula a <*> qFromFormula b
  Or a b -> QOr <$> qFromFormula a <*> qFromFormula b
  Implies a b -> QImplies <$> qFromFormula a <*> qFromFormula b
  Iff a b -> QIff <$> qFromFormula a <*> qFromFormula b
  Forall _ _ -> Nothing
  Exists _ _ -> Nothing
  Count _ _ _ -> Nothing

qEval :: (String -> [Var] -> Bool) -> (Var -> Var -> Bool)
      -> (Integer -> Var -> Bool) -> (Integer -> Bool) -> QFormula -> Bool
qEval atom equal row nullary = go where
  go q = case q of
    QTruth b -> b
    QAtom p vs -> atom p vs
    QEqual a b -> equal a b
    QNot a -> not (go a)
    QAnd a b -> go a && go b
    QOr a b -> go a || go b
    QImplies a b -> not (go a) || go b
    QIff a b -> go a == go b
    QRow i v -> row i v
    QNull i -> nullary i

rename :: (Var -> Var) -> QFormula -> QFormula
rename f q = case q of
  QTruth b -> QTruth b
  QAtom p vs -> QAtom p (map f vs)
  QEqual a b -> QEqual (f a) (f b)
  QNot a -> QNot (rename f a)
  QAnd a b -> QAnd (rename f a) (rename f b)
  QOr a b -> QOr (rename f a) (rename f b)
  QImplies a b -> QImplies (rename f a) (rename f b)
  QIff a b -> QIff (rename f a) (rename f b)
  QRow i v -> QRow i (f v)
  QNull i -> QNull i

normalize :: Checked -> Normal
normalize checked =
  let (q,Build _ rs gs us ds) = top (sentence (checkedQuery checked)) (Build 0 [] [] [] [])
  in Normal checked (reverse us) (reverse rs) (reverse gs) q (reverse ds)
  where
    top (And a b) st = let (qa,st1)=top a st; (qb,st2)=top b st1 in (QAnd qa qb,st2)
    top f@(Forall a (Forall b body)) st
      | a /= b, Just q <- qFromFormula body = addUniversal f (rename (\v -> if v==a then X else Y) q) st
    top f@(Forall a (Count b t body)) (Build i rs gs us ds)
      | a /= b, Just q <- qFromFormula body =
          (QTruth True,Build (i+1) (Row i a b t q Required:rs) gs us
            (Definition i f "equivalent-positive-constraint" "required-row":ds))
    top f@(Forall a body) st
      | Just q <- qFromFormula body = addUniversal f (rename (\v -> if v==a then X else v) q) st
    top f@(Count v t body) (Build i rs gs us ds)
      | Just q <- qFromFormula body =
          (QTruth True,Build (i+1) rs (Global i v t q Required:gs) us
            (Definition i f "equivalent-positive-constraint" "required-global":ds))
    top f st = lower f st
    addUniversal f q (Build i rs gs us ds) = (QTruth True,Build (i+1) rs gs (q:us)
      (Definition i f "equivalent-positive-constraint" "universal":ds))

    lower f st = case f of
      Truth b -> (QTruth b,st)
      Atom p vs -> (QAtom p vs,st)
      Equal a b -> (QEqual a b,st)
      Not a -> let (qa,st1)=lower a st in (QNot qa,st1)
      And a b -> two QAnd a b st
      Or a b -> two QOr a b st
      Implies a b -> two QImplies a b st
      Iff a b -> two QIff a b st
      Forall v a -> quantified f v (Ordinary CEq 0) (Not a) st
      Exists v a -> quantified f v (Ordinary CGe 1) a st
      Count v t a -> quantified f v t a st
    two ctor a b st = let (qa,st1)=lower a st; (qb,st2)=lower b st1 in (ctor qa qb,st2)
    quantified original v t body st =
      let (q,Build i rs gs us ds)=lower body st
          remaining=S.toList (freeVars original)
          def role=Definition i original "unique-definitional-extension" role
      in case remaining of
        [] -> (QNull i,Build (i+1) rs (Global i v t q Annotation:gs) us (def "nullary":ds))
        w:_ -> (QRow i w,Build (i+1) (Row i w v t q Annotation:rs) gs us (def "unary":ds))

-- Used for replay/introspection; IDs are dependency ranks, not human approval.
normalIdentities :: Normal -> M.Map Integer String
normalIdentities n = M.fromList [(definitionId d,definitionRelation d) | d <- definitions n]
