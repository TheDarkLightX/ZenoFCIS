{-# LANGUAGE BangPatterns #-}
module Zal.Logic.Budget
  ( Budget, Stats(..), runBudget, liftEither, askLimits, charge, checkSize
  , addMass, multiplyMass, sumMass, powerCheck
  ) where
import qualified Data.Map.Strict as M
import Zal.Logic.Syntax

data Stats = Stats { work :: !Integer, peakEntries :: !Integer, peakBits :: !Integer }
  deriving (Eq,Show)
newtype Budget a = Budget (Limits -> Stats -> Either Failure (a,Stats))
instance Functor Budget where
  fmap f (Budget g) = Budget (\l s -> do (a,s')<-g l s; Right (f a,s'))
instance Applicative Budget where
  pure a = Budget (\_ s -> Right (a,s))
  Budget f <*> Budget g = Budget (\l s -> do (h,s')<-f l s; (a,s'')<-g l s'; Right (h a,s''))
instance Monad Budget where
  Budget f >>= k = Budget (\l s -> do (a,s')<-f l s; let Budget g=k a in g l s')
runBudget :: Limits -> Budget a -> Either Failure (a,Stats)
runBudget lim (Budget f) = f lim (Stats 0 0 0)
liftEither :: Either Failure a -> Budget a
liftEither e = Budget (\_ s -> do a<-e; Right (a,s))
askLimits :: Budget Limits
askLimits = Budget (\l s -> Right (l,s))
charge :: Integer -> Budget ()
charge n = Budget (\l s -> let w=work s+n in
  if w>maxWork l then Left (ResourceExhausted "transition work" w (maxWork l))
  else Right ((),s {work=w}))
checkSize :: Integer -> Budget ()
checkSize n = Budget (\l s -> if n>maxEntries l
  then Left (ResourceExhausted "map entries" n (maxEntries l))
  else Right ((),s {peakEntries=max n (peakEntries s)}))
checkMass :: Rational -> Budget Rational
checkMass !r = Budget (\l s -> let bits=rationalBits r in
  if bits>maxBits l then Left (ResourceExhausted "rational bits" bits (maxBits l))
  else Right (r,s {peakBits=max bits (peakBits s)}))
multiplyMass :: Rational -> Rational -> Budget Rational
multiplyMass a b = charge 1 >> checkMass (a*b)
sumMass :: Rational -> Rational -> Budget Rational
sumMass a b = charge 1 >> checkMass (a+b)
addMass :: Ord k => k -> Rational -> M.Map k Rational -> Budget (M.Map k Rational)
addMass key mass table = do
  total <- sumMass mass (M.findWithDefault 0 key table)
  let !updated = if total==0 then M.delete key table else M.insert key total table
  checkSize (toInteger (M.size updated))
  pure updated
powerCheck :: Integer -> Budget Integer
powerCheck bits = do lim<-askLimits; liftEither (boundedPower (maxCandidates lim) bits)
