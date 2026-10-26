{-# LANGUAGE BangPatterns #-}
module Zal.Logic.Syntax
  ( Var(..), Arity(..), Cmp(..), Test(..), Formula(..), Signature, Weight(..)
  , Profile(..), Query(..), Checked, checkQuery, checkedQuery, freeVars
  , Failure(..), Limits(..), defaultLimits, testCount, observationFor
  , Observation(..), observe, addObserved, rationalBits, boundedPower
  ) where

import Data.Char (isAsciiLower, isAsciiUpper, isDigit)
import qualified Data.Map.Strict as M
import qualified Data.Set as S
import Data.Ratio (numerator, denominator)

data Var = X | Y deriving (Eq, Ord, Show)
data Arity = Unary | Binary deriving (Eq, Ord, Show)
data Cmp = CEq | CLe | CGe deriving (Eq, Ord, Show)
data Test = Ordinary !Cmp !Integer | Modular !Cmp !Integer !Integer
  deriving (Eq, Ord, Show)
data Formula
  = Truth !Bool | Atom !String ![Var] | Equal !Var !Var
  | Not !Formula | And !Formula !Formula | Or !Formula !Formula
  | Implies !Formula !Formula | Iff !Formula !Formula
  | Forall !Var !Formula | Exists !Var !Formula | Count !Var !Test !Formula
  deriving (Eq, Ord, Show)
type Signature = M.Map String Arity
data Weight = Weight { trueWeight :: !Rational, falseWeight :: !Rational }
  deriving (Eq, Show)
data Profile = C2 | C2mod deriving (Eq, Show)
data Query = Query
  { profile :: !Profile, signature :: !Signature, sentence :: !Formula
  , domainSize :: !Integer, weights :: !(M.Map String Weight)
  } deriving (Eq, Show)
newtype Checked = Checked Query deriving (Eq, Show)
checkedQuery :: Checked -> Query
checkedQuery (Checked q) = q
data Failure = Invalid !String | Unsupported !String
  | ResourceExhausted !String !Integer !Integer | InternalDisagreement !String
  deriving (Eq, Show)
data Limits = Limits
  { maxEntries :: !Integer, maxCandidates :: !Integer, maxWork :: !Integer
  , maxBits :: !Integer, maxDomain :: !Integer
  } deriving (Eq, Show)
defaultLimits :: Limits
defaultLimits = Limits 100000 1000000 10000000 65536 10000

freeVars :: Formula -> S.Set Var
freeVars f = case f of
  Truth _ -> S.empty
  Atom _ vs -> S.fromList vs
  Equal a b -> S.fromList [a,b]
  Not a -> freeVars a
  And a b -> both a b
  Or a b -> both a b
  Implies a b -> both a b
  Iff a b -> both a b
  Forall v a -> S.delete v (freeVars a)
  Exists v a -> S.delete v (freeVars a)
  Count v _ a -> S.delete v (freeVars a)
  where both a b = S.union (freeVars a) (freeVars b)

checkQuery :: Limits -> Query -> Either Failure Checked
checkQuery lim q = do
  if any (<=0) [maxEntries lim,maxCandidates lim,maxWork lim,maxBits lim,maxDomain lim]
    then Left (Invalid "limits must be positive") else Right ()
  if domainSize q <= 0 then Left (Invalid "domain must be nonempty") else Right ()
  if domainSize q > maxDomain lim
    then Left (ResourceExhausted "domain" (domainSize q) (maxDomain lim)) else Right ()
  if M.size (signature q) > 128 then Left (Invalid "signature exceeds 128 declarations") else Right ()
  if all validId (M.keys (signature q)) then Right () else Left (Invalid "invalid predicate id")
  if M.keysSet (signature q) /= M.keysSet (weights q)
    then Left (Invalid "weights must cover the complete signature exactly") else Right ()
  mapM_ checkWeight (M.elems (weights q))
  -- Explicit work list: validate both children, even in a dead Boolean branch.
  walk 0 [(sentence q,S.empty,1)]
  Right (Checked q)
  where
    validId [] = False
    validId (c:cs) = letter c && length cs < 128 && all (\a -> letter a || isDigit a || a=='_') cs
    letter c = isAsciiLower c || isAsciiUpper c
    checkWeight (Weight a b) = mapM_ (\r -> if rationalBits r > maxBits lim
      then Left (ResourceExhausted "weight bits" (rationalBits r) (maxBits lim)) else Right ()) [a,b]
    walk :: Integer -> [(Formula,S.Set Var,Integer)] -> Either Failure ()
    walk _ [] = Right ()
    walk n ((f,bound,depth):rest)
      | n >= 10000 = Left (ResourceExhausted "formula nodes" (n+1) 10000)
      | depth > 256 = Left (ResourceExhausted "formula depth" depth 256)
      | otherwise = do
          let child a = (a,bound,depth+1)
              quantified v a = (a,S.insert v bound,depth+1)
              boundVars vs = if all (`S.member` bound) vs then Right () else Left (Invalid "unbound variable")
          next <- case f of
            Truth _ -> Right []
            Atom p vs -> do
              ar <- maybe (Left (Invalid ("undeclared predicate: "++p))) Right (M.lookup p (signature q))
              if length vs == (case ar of Unary -> 1; Binary -> 2)
                then boundVars vs else Left (Invalid "predicate arity mismatch")
              Right []
            Equal a b -> boundVars [a,b] >> Right []
            Not a -> Right [child a]
            And a b -> Right [child a,child b]
            Or a b -> Right [child a,child b]
            Implies a b -> Right [child a,child b]
            Iff a b -> Right [child a,child b]
            Forall v a -> Right [quantified v a]
            Exists v a -> Right [quantified v a]
            Count v t a -> checkTest t >> Right [quantified v a]
          walk (n+1) (next++rest)
    checkTest (Ordinary _ k)
      | k < 0 = Left (Invalid "negative threshold")
      | otherwise = checkNumeral k
    checkTest (Modular _ r m)
      | profile q /= C2mod = Left (Unsupported "modular counting needs zal/c2mod/1")
      | m < 1 || r < 0 || r >= m = Left (Invalid "modulus or residue out of range")
      | otherwise = checkNumeral r >> checkNumeral m
    checkNumeral k = if integerBits k > 851 then Left (Invalid "semantic numeral too large") else Right ()

compareCount :: Cmp -> Integer -> Integer -> Bool
compareCount CEq = (==)
compareCount CLe = (<=)
compareCount CGe = (>=)
testCount :: Test -> Integer -> Bool
testCount (Ordinary c k) a = compareCount c a k
testCount (Modular c r m) a = m > 0 && compareCount c (a `mod` m) r

data Observation = Strict !Integer | Saturated !Integer | Residue !Integer
  deriving (Eq, Ord, Show)
observationFor :: Bool -> Test -> Observation
observationFor strict t = case t of
  Ordinary CGe k -> Saturated k
  Ordinary _ k -> if strict then Strict k else Saturated (k+1)
  Modular _ _ m -> Residue m
observe :: Observation -> Integer -> Maybe Integer
observe _ n | n < 0 = Nothing
observe (Strict k) n | k >= 0 && n <= k = Just n
observe (Saturated k) n | k >= 0 = Just (min k n)
observe (Residue m) n | m > 0 = Just (n `mod` m)
observe _ _ = Nothing
addObserved :: [Observation] -> [Integer] -> [Integer] -> Maybe [Integer]
addObserved [] [] [] = Just []
addObserved (o:os) (a:as) (b:bs) = do
  -- Existing observations must themselves be canonical.
  a' <- observe o a
  b' <- observe o b
  if a' /= a || b' /= b then Nothing else do
    c <- observe o (a+b)
    cs <- addObserved os as bs
    Just (c:cs)
addObserved _ _ _ = Nothing

integerBits :: Integer -> Integer
integerBits n = go (abs n) 0 where
  go 0 !acc = acc
  go a !acc = go (a `quot` 2) (acc+1)
rationalBits :: Rational -> Integer
rationalBits r = max (integerBits (numerator r)) (integerBits (denominator r))
boundedPower :: Integer -> Integer -> Either Failure Integer
boundedPower cap power = go 1 power where
  go a 0 = Right a
  go a e | e < 0 = Left (Invalid "negative power")
         | a > cap `quot` 2 = Left (ResourceExhausted "Boolean assignments" (cap+1) cap)
         | otherwise = go (a*2) (e-1)
