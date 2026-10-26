module Zal.Logic.Finite
  ( Structure, structure, structureRelations, evaluate, evaluateWith
  , evaluateBounded, evaluationWork, weightOf, enumerateStructures, atomAt ) where

import Control.Monad (foldM, replicateM)
import qualified Data.Map.Strict as M
import qualified Data.Set as S
import Zal.Logic.Syntax

data Structure = Structure !Integer !Signature !(M.Map String (S.Set [Integer])) deriving (Eq,Show)

structureRelations :: Structure -> M.Map String (S.Set [Integer])
structureRelations (Structure _ _ tables) = tables

-- Sets give the complete true extension; every other in-domain tuple is false.
structure :: Checked -> M.Map String (S.Set [Integer]) -> Either Failure Structure
structure c tables = do
  let q = checkedQuery c
  if M.keysSet tables /= M.keysSet (signature q) then Left (Invalid "model vocabulary mismatch") else Right ()
  mapM_ (checkTable q) (M.toList tables)
  Right (Structure (domainSize q) (signature q) tables)
  where
    checkTable q (p,ts) = case M.lookup p (signature q) of
      Nothing -> Left (Invalid "model predicate missing")
      Just ar -> if all (valid ar (domainSize q)) (S.toList ts) then Right () else Left (Invalid "model tuple out of domain or arity")
    valid ar n tuple = length tuple == (case ar of Unary -> 1; Binary -> 2) && all (\x -> x>=0 && x<n) tuple

evaluate :: Checked -> Structure -> Either Failure Bool
evaluate c m = evaluateWith c m M.empty (sentence (checkedQuery c))

-- Conservative number of formula visits, counting both Boolean branches and
-- every quantified entity. This is an abstract work bound, not a CPU theorem.
evaluationWork :: Integer -> Formula -> Integer
evaluationWork n f = 1 + case f of
  Truth _ -> 0
  Atom _ _ -> 0
  Equal _ _ -> 0
  Not a -> one a
  And a b -> two a b
  Or a b -> two a b
  Implies a b -> two a b
  Iff a b -> two a b
  Forall _ a -> n * one a
  Exists _ a -> n * one a
  Count _ _ a -> n * one a
  where
    one = evaluationWork n
    two a b = one a + one b

evaluateBounded :: Limits -> Checked -> Structure -> Either Failure (Bool,Integer)
evaluateBounded lim c m = do
  let q=checkedQuery c
      bound=evaluationWork (domainSize q) (sentence q)
  if bound>maxWork lim then Left (ResourceExhausted "formula visits" bound (maxWork lim)) else Right ()
  result<-evaluate c m
  Right (result,bound)

evaluateWith :: Checked -> Structure -> M.Map Var Integer -> Formula -> Either Failure Bool
evaluateWith c (Structure n sig tables) env f
  | n /= domainSize q || sig /= signature q = Left (Invalid "structure bound to a different domain/signature")
  | any (\a -> a<0 || a>=n) (M.elems env) = Left (Invalid "environment outside domain")
  | otherwise = go env f
  where
  q = checkedQuery c
  domain = [0..domainSize q-1]
  value rho v = maybe (Left (Invalid "missing environment binding")) Right (M.lookup v rho)
  go rho node = case node of
    Truth b -> Right b
    Atom p vs -> do
      args <- mapM (value rho) vs
      ts <- maybe (Left (Invalid "missing interpretation")) Right (M.lookup p tables)
      Right (S.member args ts)
    Equal a b -> (==) <$> value rho a <*> value rho b
    Not a -> not <$> go rho a
    And a b -> (&&) <$> go rho a <*> go rho b
    Or a b -> (||) <$> go rho a <*> go rho b
    Implies a b -> (\x y -> not x || y) <$> go rho a <*> go rho b
    Iff a b -> (==) <$> go rho a <*> go rho b
    Forall v a -> and <$> mapM (\x -> go (M.insert v x rho) a) domain
    Exists v a -> or <$> mapM (\x -> go (M.insert v x rho) a) domain
    Count v t a -> do
      cardinality <- foldM (\acc x -> do b <- go (M.insert v x rho) a; Right (acc+if b then 1 else 0)) 0 domain
      Right (testCount t cardinality)

weightOf :: Checked -> Structure -> Rational
weightOf c (Structure _ _ tables) = product
  [ trueWeight w ^ trueN * falseWeight w ^ (total-trueN)
  | (p,ar) <- M.toList (signature q), Just w <- [M.lookup p (weights q)]
  , let trueN = toInteger (S.size (M.findWithDefault S.empty p tables))
        total = domainSize q ^ (case ar of Unary -> (1::Integer); Binary -> 2)]
  where q = checkedQuery c

enumerateStructures :: Limits -> Checked -> Either Failure [Structure]
enumerateStructures lim c = do
  let q = checkedQuery c
      n = domainSize q
      atoms = [(p,tuple) | (p,ar) <- M.toList (signature q), tuple <- replicateM (case ar of Unary -> 1; Binary -> 2) [0..n-1]]
      atomCount = sum [n ^ (case ar of Unary -> (1::Integer); Binary -> 2) | ar <- M.elems (signature q)]
  _ <- boundedPower (maxCandidates lim) atomCount
  let empty = M.map (const S.empty) (signature q)
      make bits = Structure n (signature q) (foldl (\m ((p,t),b) -> if b then M.adjust (S.insert t) p m else m) empty (zip atoms bits))
  Right (map make (replicateM (length atoms) [False,True]))

atomAt :: Structure -> String -> [Integer] -> Bool
atomAt (Structure _ _ tables) p args = S.member args (M.findWithDefault S.empty p tables)
