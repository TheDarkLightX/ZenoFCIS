{-# LANGUAGE BangPatterns #-}
module Zal.Logic.Population (countModels, countNormal) where

import Control.Monad (foldM, replicateM)
import qualified Data.Map.Strict as M
import Zal.Logic.Syntax
import Zal.Logic.Normalize
import Zal.Logic.Budget

data OneType = OneType
  { unaryBits :: !(M.Map String Bool), diagonalBits :: !(M.Map String Bool)
  , annotationBits :: !(M.Map Integer Bool), typeMass :: !Rational
  } deriving (Eq,Show)
type Pair = M.Map (String,Bool) Bool
type Local = (Integer,[Integer])
type Population = M.Map Local Integer
type Histogram = M.Map [Integer] Integer
type GroupDist = M.Map ([Integer],Histogram) Rational
type PairDist = M.Map ([Integer],[Integer]) Rational
type PairIndex = M.Map (Integer,Integer) PairDist

bit :: Bool -> Integer
bit False = 0
bit True = 1

countModels :: Limits -> Checked -> Either Failure (Rational,Stats)
countModels lim checked = countNormal lim (normalize checked)

countNormal :: Limits -> Normal -> Either Failure (Rational,Stats)
countNormal lim normal = runBudget lim $ do
  let gs=[globalId g | g<-globals normal, globalObligation g==Annotation]
  _ <- powerCheck (toInteger (length gs))
  foldM (branch gs) 0 (replicateM (length gs) [False,True])
  where
    branch gs total bs = do
      charge 1
      let nulls=M.fromList (zip gs bs)
          rootOK=qEval (\_ _->False) (==) (\_ _->False) (lookupNull nulls) (root normal)
      if not rootOK then pure total else do
        types <- makeTypes normal nulls
        pairs <- makePairs normal nulls types
        let layout=map (\r -> observationFor (rowObligation r==Required) (rowTest r)) (rows normal)
            initial=M.singleton M.empty 1
        final <- stages normal nulls types pairs layout 0 initial
        answer <- foldM (\a (pop,w) -> if finalOK normal nulls types pop then sumMass a w else pure a) 0 (M.toList final)
        sumMass total answer

lookupNull :: M.Map Integer Bool -> Integer -> Bool
lookupNull = flip (M.findWithDefault False)

-- Bind variables to the two current labels. False names the new/left label.
interpret :: M.Map Integer Bool -> OneType -> OneType -> Pair -> Bool
          -> (Var -> Bool) -> QFormula -> Bool
interpret nulls left right pair diagonal env = qEval atom equal ann (lookupNull nulls) where
  at v = if env v then right else left
  equal a b = diagonal || env a==env b
  ann i v = M.findWithDefault False i (annotationBits (at v))
  atom p [v] = M.findWithDefault False p (unaryBits (at v))
  atom p [a,b]
    | equal a b = M.findWithDefault False p (diagonalBits (at a))
    | otherwise = M.findWithDefault False (p,env a) pair
  atom _ _ = False -- unreachable after checked syntax admission

standard :: Var -> Bool
standard X = False
standard Y = True

makeTypes :: Normal -> M.Map Integer Bool -> Budget (M.Map Integer OneType)
makeTypes normal nulls = do
  let q=checkedQuery (source normal)
      us=[p | (p,Unary)<-M.toList (signature q)]
      bs=[p | (p,Binary)<-M.toList (signature q)]
      as=[rowId r | r<-rows normal, rowObligation r==Annotation]
      keys=length us+length bs+length as
  _ <- powerCheck (toInteger keys)
  built <- foldM (candidate q us bs as) [] (replicateM keys [False,True])
  pure (M.fromList (zip [0..] (reverse built)))
  where
    candidate q us bs as acc bits = do
      charge 1
      let (u,rest)=splitAt (length us) bits
          (d,a)=splitAt (length bs) rest
          ty=OneType (M.fromList (zip us u)) (M.fromList (zip bs d)) (M.fromList (zip as a)) 1
      if all (interpret nulls ty ty M.empty True standard) (universal normal) then do
        w <- foldM (\mass (p,b) -> case M.lookup p (weights q) of
          Nothing -> liftEither (Left (InternalDisagreement "type weight absent"))
          Just pair -> multiplyMass mass (if b then trueWeight pair else falseWeight pair)) 1 (zip (us++bs) (u++d))
        checkSize (toInteger (length acc)+1)
        pure (ty {typeMass=w}:acc)
      else pure acc

makePairs :: Normal -> M.Map Integer Bool -> M.Map Integer OneType -> Budget PairIndex
makePairs normal nulls types = do
  let bs=[p | (p,Binary)<-M.toList (signature (checkedQuery (source normal)))]
      keys=[(p,d) | p<-bs,d<-[False,True]]
  choices <- powerCheck (toInteger (length keys))
  lim<-askLimits
  let total=toInteger (M.size types)^ (2::Integer) * choices
  if total>maxCandidates lim then liftEither (Left (ResourceExhausted "pair candidates" total (maxCandidates lim))) else pure ()
  foldM (onePair keys) M.empty [(i,a,j,b) | (i,a)<-M.toList types,(j,b)<-M.toList types]
  where
    onePair keys acc (i,a,j,b) = do
      dist<-foldM (table a b keys) M.empty (replicateM (length keys) [False,True])
      let updated=M.insert (i,j) dist acc
      checkSize (toInteger (sum (map M.size (M.elems updated))))
      pure updated
    table a b keys acc bits = do
      charge 1
      let pair=M.fromList (zip keys bits)
          eval=interpret nulls a b pair False
          allowed=all (eval standard) (universal normal) && all (eval (not . standard)) (universal normal)
          increments reverseSide = [bit (eval (\v -> if v==owner r then reverseSide else not reverseSide) (rowBody r)) | r<-rows normal]
      if not allowed then pure acc else do
        w<-foldM (\mass ((p,_),bval) -> case M.lookup p (weights (checkedQuery (source normal))) of
          Nothing -> liftEither (Left (InternalDisagreement "pair weight absent"))
          Just wp -> multiplyMass mass (if bval then trueWeight wp else falseWeight wp)) 1 (zip keys bits)
        addMass (increments False,increments True) w acc

observed :: [Observation] -> [Integer] -> Maybe [Integer]
observed [] [] = Just []
observed (o:os) (a:as) = (:) <$> observe o a <*> observed os as
observed _ _ = Nothing
addRaw :: [Observation] -> [Integer] -> [Integer] -> Maybe [Integer]
addRaw layout a b = observed layout b >>= addObserved layout a

group :: [Observation] -> PairDist -> Integer -> Budget GroupDist
group layout options size = loop 0 (M.singleton (replicate (length layout) 0,M.empty) 1) where
  loop h dist
    | h==size = pure dist
    | otherwise = do
        next<-foldM extend M.empty [(u,hist,w,t,s,r) | ((u,hist),w)<-M.toList dist,((t,s),r)<-M.toList options]
        loop (h+1) next
  extend acc (u,hist,w,t,s,r) = do
    charge 1
    case addRaw layout u t of
      Nothing -> pure acc
      Just next -> do
        mass<-multiplyMass w r
        addMass (next,M.insertWith (+) s 1 hist) mass acc

moveGroup :: [Observation] -> Local -> Integer -> Histogram -> Either Failure (Maybe Population)
moveGroup layout (j,c) size hist
  | sum (M.elems hist) /= size = Left (InternalDisagreement "group histogram lost population")
  | otherwise = Right (foldM step M.empty (M.toList hist))
  where
    step acc (s,d) = do
      new<-addRaw layout c s
      Just (M.insertWith (+) (j,new) d acc)

stages :: Normal -> M.Map Integer Bool -> M.Map Integer OneType -> PairIndex -> [Observation]
       -> Integer -> M.Map Population Rational -> Budget (M.Map Population Rational)
stages normal nulls types pairs layout h dist
  | h==domainSize (checkedQuery (source normal)) = pure dist
  | M.null dist = pure M.empty
  | otherwise = do
      next<-foldM extend M.empty [(pop,w,i,ty) | (pop,w)<-M.toList dist,(i,ty)<-M.toList types]
      stages normal nulls types pairs layout (h+1) next
  where
    extend acc (pop,w,i,ty) = do
      let eval=interpret nulls ty ty M.empty True (const False)
          diagonal=map (bit . eval . rowBody) (rows normal)
          zero=replicate (length layout) 0
      case observed layout diagonal of
        Nothing -> pure acc
        Just diag -> do
          combined<-foldM (combine i) (M.singleton (zero,M.empty) 1) (M.toList pop)
          foldM (insertNew w i ty diag) acc (M.toList combined)
    combine i partial (old@(j,_),d) = do
      options<-maybe (liftEither (Left (InternalDisagreement "missing pair distribution"))) pure (M.lookup (i,j) pairs)
      gd<-group layout options d
      foldM (merge old d) M.empty [(a,oldpop,w,u,hist,r) | ((a,oldpop),w)<-M.toList partial,((u,hist),r)<-M.toList gd]
    merge old d acc (a,oldpop,w,u,hist,r) = do
      charge 1
      moved<-liftEither (moveGroup layout old d hist)
      case (addObserved layout a u,moved) of
        (Just next,Just relocated) -> do
          mass<-multiplyMass w r
          addMass (next,M.unionWith (+) oldpop relocated) mass acc
        _ -> pure acc
    insertNew w i ty diag acc ((u,oldpop),coefficient)
      | sum (M.elems oldpop) /= h = liftEither (Left (InternalDisagreement "population not conserved"))
      | otherwise = case addObserved layout diag u of
          Nothing -> pure acc
          Just c -> do
            mass<-multiplyMass w (typeMass ty) >>= (`multiplyMass` coefficient)
            addMass (M.insertWith (+) (i,c) 1 oldpop) mass acc

finalOK :: Normal -> M.Map Integer Bool -> M.Map Integer OneType -> Population -> Bool
finalOK normal nulls types pop = all rowOK (M.toList pop) && all globalOK (globals normal) where
  rowOK ((i,counts),d) = d>0 && case M.lookup i types of
    Nothing -> False
    Just ty -> and [case rowObligation r of
      Required -> testCount (rowTest r) c
      Annotation -> M.lookup (rowId r) (annotationBits ty)==Just (testCount (rowTest r) c)
      | (r,c)<-zip (rows normal) counts] && length counts==length (rows normal)
  globalOK g =
    let cardinality=sum [d | ((i,_),d)<-M.toList pop,Just ty<-[M.lookup i types]
                          ,interpret nulls ty ty M.empty True (const False) (globalBody g)]
        result=testCount (globalTest g) cardinality
    in case globalObligation g of
      Required -> result
      Annotation -> M.lookup (globalId g) nulls==Just result
