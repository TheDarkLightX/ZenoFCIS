module Zal.Lens.FiniteCategory
  ( RawCategory(..), RawArrow(..), RawFunctor(..), Category, FunctorMap
  , Fault(..), Check, runCheck, charge, invalid, refute, require
  , checkCategory, checkFunctor, composeFunctors, inverse, pathComposite
  , categoryId, categoryObjects, categoryArrows, functorData, fibers
  ) where

import Control.Monad (foldM, unless, forM_)
import Data.Char (isControl)
import Data.List (sort)
import qualified Data.Map.Strict as M
import qualified Data.Set as S
import Zal.Lens.Typed (atom, joinChain)

data RawArrow = RawArrow { arrowId :: String, source :: String, target :: String } deriving (Eq,Show)
data RawCategory = RawCategory
  { rawCategoryId :: String, rawObjects :: [String], rawArrows :: [RawArrow]
  , rawIdentities :: [(String,String)], rawComposition :: [((String,String),String)]
  } deriving (Eq,Show)
data RawFunctor = RawFunctor
  { rawFunctorId :: String, fromCategory :: String, toCategory :: String
  , objectMap :: [(String,String)], arrowMap :: [(String,String)]
  } deriving (Eq,Show)
data Category = Category RawCategory (M.Map String RawArrow) (M.Map String String) (M.Map (String,String) String)
data FunctorMap = FunctorMap RawFunctor Category Category

data Fault = Invalid String | Unknown String | Refuted String [(String,String)] deriving (Eq,Show)
newtype Check a = Check (Int -> Int -> Either (Fault,Int) (a,Int))
instance Functor Check where
  fmap f (Check g) = Check (\cap used -> do (a,next) <- g cap used; pure (f a,next))
instance Applicative Check where
  pure a = Check (\_ used -> Right (a,used))
  Check f <*> Check g = Check (\cap used -> do (h,n) <- f cap used; (a,m) <- g cap n; pure (h a,m))
instance Monad Check where
  Check f >>= k = Check (\cap used -> do (a,next) <- f cap used; let Check g = k a in g cap next)

runCheck :: Int -> Check a -> Either (Fault,Int) (a,Int)
runCheck cap (Check f) = f cap 0

raise :: Fault -> Check a
raise fault = Check (\_ used -> Left (fault,used))
charge :: Check ()
charge = Check (\cap used -> if used >= cap then Left (Unknown "work exhausted; next charged visit not performed",used)
                           else Right ((),used+1))
invalid :: String -> Check a
invalid = raise . Invalid
refute :: String -> [(String,String)] -> Check a
refute law = raise . Refuted law
require :: String -> [(String,String)] -> Bool -> Check ()
require law witness condition = charge >> unless condition (refute law witness)

validId :: String -> Bool
validId value = not (null value) && length value <= 128 && not (any isControl value)
unique :: Ord a => [a] -> Bool
unique values = length values == S.size (S.fromList values)
shape :: String -> Bool -> Check ()
shape message condition = charge >> unless condition (invalid message)

categoryId :: Category -> String
categoryId (Category raw _ _ _) = rawCategoryId raw
categoryObjects :: Category -> [String]
categoryObjects (Category raw _ _ _) = rawObjects raw
categoryArrows :: Category -> [RawArrow]
categoryArrows (Category raw _ _ _) = rawArrows raw
functorData :: FunctorMap -> RawFunctor
functorData (FunctorMap raw _ _) = raw

lookupChecked :: Ord k => String -> k -> M.Map k a -> Check a
lookupChecked message key table = maybe (invalid message) pure (M.lookup key table)
arrow :: Category -> String -> Check RawArrow
arrow (Category _ table _ _) ident = lookupChecked "unknown arrow" ident table
identity :: Category -> String -> Check String
identity (Category _ _ table _) ident = lookupChecked "missing identity" ident table
compose :: Category -> String -> String -> Check String
compose (Category _ _ _ table) before after = lookupChecked "missing checked composition" (before,after) table

checkCategory :: RawCategory -> Check Category
checkCategory raw = do
  let os = rawObjects raw; as = rawArrows raw; ids = map arrowId as
      im = M.fromList (rawIdentities raw); cm = M.fromList (rawComposition raw)
      category = Category raw (M.fromList [(arrowId a,a) | a <- as]) im cm
  shape "invalid category/object/arrow bounds or identifiers"
    (validId (rawCategoryId raw) && not (null os) && length os <= 8 && not (null as) && length as <= 48
     && all validId (os ++ ids) && unique os && unique ids && length (rawComposition raw) <= 2304)
  shape "identity/composition declarations duplicate or omit keys"
    (unique (map fst (rawIdentities raw)) && sort (M.keys im) == sort os
     && unique (map fst (rawComposition raw)))
  forM_ as $ \a -> shape "arrow endpoint outside object set" (source a `elem` os && target a `elem` os)
  forM_ (rawIdentities raw) $ \(o,i) -> do
    a <- arrow category i
    require "identity-endpoints" [("object",o),("identity",i)] (source a == o && target a == o)
  forM_ (rawComposition raw) $ \((f,g),h) -> do
    a <- arrow category f; b <- arrow category g; c <- arrow category h
    require "composition-typing" [("before",f),("after",g),("result",h)]
      (target a == source b && source c == source a && target c == target b)
  forM_ as $ \a -> forM_ as $ \b -> do
    charge
    if target a /= source b then pure () else
      unless (M.member (arrowId a,arrowId b) cm)
        (refute "composition-total" [("before",arrowId a),("after",arrowId b)])
  forM_ as $ \a -> do
    left <- identity category (source a); right <- identity category (target a)
    l <- compose category left (arrowId a); r <- compose category (arrowId a) right
    require "identity-laws" [("arrow",arrowId a),("left",l),("right",r)] (l == arrowId a && r == arrowId a)
  forM_ as $ \a -> forM_ as $ \b -> forM_ as $ \c -> do
    charge
    unless (target a /= source b || target b /= source c) $ do
      ab <- compose category (arrowId a) (arrowId b); bc <- compose category (arrowId b) (arrowId c)
      left <- compose category ab (arrowId c); right <- compose category (arrowId a) bc
      require "associativity" [("first",arrowId a),("second",arrowId b),("third",arrowId c),("left",left),("right",right)] (left == right)
  pure category

checkFunctor :: M.Map String Category -> RawFunctor -> Check FunctorMap
checkFunctor categories raw = do
  charge
  s <- lookupChecked "unknown source category" (fromCategory raw) categories
  t <- lookupChecked "unknown target category" (toCategory raw) categories
  let om = M.fromList (objectMap raw); am = M.fromList (arrowMap raw)
  shape "functor mappings must be total and unique"
    (validId (rawFunctorId raw) && unique (map fst (objectMap raw)) && unique (map fst (arrowMap raw))
     && sort (M.keys om) == sort (categoryObjects s) && sort (M.keys am) == sort (map arrowId (categoryArrows s)))
  forM_ (objectMap raw) $ \(o,p) -> do
    shape "functor object outside target" (p `elem` categoryObjects t)
    i <- identity s o; mapped <- lookupChecked "identity mapping missing" i am; expected <- identity t p
    require "functor-identity" [("functor",rawFunctorId raw),("object",o),("actual",mapped),("expected",expected)] (mapped == expected)
  forM_ (categoryArrows s) $ \a -> do
    ident <- lookupChecked "missing arrow mapping" (arrowId a) am; b <- arrow t ident
    p <- lookupChecked "missing object mapping" (source a) om; q <- lookupChecked "missing object mapping" (target a) om
    require "functor-endpoints" [("functor",rawFunctorId raw),("arrow",arrowId a),("image",ident)] (source b == p && target b == q)
  forM_ (categoryArrows s) $ \a -> forM_ (categoryArrows s) $ \b -> do
    charge
    unless (target a /= source b) $ do
      ab <- compose s (arrowId a) (arrowId b)
      fa <- lookupChecked "missing map" (arrowId a) am; fb <- lookupChecked "missing map" (arrowId b) am
      left <- lookupChecked "missing composite map" ab am; right <- compose t fa fb
      require "functor-composition" [("functor",rawFunctorId raw),("before",arrowId a),("after",arrowId b),("actual",left),("expected",right)] (left == right)
  pure (FunctorMap raw s t)

composeFunctors :: M.Map String Category -> [FunctorMap] -> Check FunctorMap
composeFunctors _ [] = invalid "empty functor chain"
composeFunctors categories (first:rest) = foldM step first rest where
  step left right = do
    charge
    let a = functorData left; b = functorData right
    case joinChain (atom (rawFunctorId a) (fromCategory a) (toCategory a))
                   (atom (rawFunctorId b) (fromCategory b) (toCategory b)) of
      Left message -> refute "typed-composition" [("detail",message)]
      Right _ -> pure ()
    let bm = M.fromList (objectMap b); ba = M.fromList (arrowMap b)
    os <- mapM (\(o,p) -> do q <- lookupChecked "object composition missing" p bm; pure (o,q)) (objectMap a)
    as <- mapM (\(f,g) -> do h <- lookupChecked "arrow composition missing" g ba; pure (f,h)) (arrowMap a)
    -- Recompute laws even though the symbolic endpoints type-check.
    checkFunctor categories (RawFunctor "composite" (fromCategory a) (toCategory b) os as)

inverse :: Category -> String -> Check String
inverse category ident = do
  charge
  a <- arrow category ident
  ia <- identity category (source a); ib <- identity category (target a)
  let search [] = refute "invertible" [("arrow",ident),("reason","no two-sided inverse in complete declared arrow set")]
      search (b:bs) = do
        charge
        if source b /= target a || target b /= source a then search bs else do
          left <- compose category ident (arrowId b); right <- compose category (arrowId b) ident
          if left == ia && right == ib then pure (arrowId b) else search bs
  search (categoryArrows category)

pathComposite :: Category -> [String] -> Check String
pathComposite _ [] = invalid "empty arrow path"
pathComposite category (first:rest) = do
  a <- arrow category first
  (_,result) <- foldM step (atom first (source a) (target a),first) rest
  pure result
  where
    step (chain,prior) next = do
      charge
      a <- arrow category next
      joined <- case joinChain chain (atom next (source a) (target a)) of
        Left message -> refute "typed-composition" [("detail",message)]
        Right value -> pure value
      result <- compose category prior next
      pure (joined,result)

fibers :: [(String,String)] -> [(String,[String])]
fibers pairs = M.toList (M.fromListWith (++) [(image,[original]) | (original,image) <- pairs])
