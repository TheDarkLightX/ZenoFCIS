{-# LANGUAGE DataKinds, GADTs, KindSignatures, ScopedTypeVariables, TypeOperators #-}
-- Typed syntax, not a proof of laws, denotation, invertibility or termination.
module Zal.Lens.Typed (Chain, SomeChain, named, thenChain, atom, joinChain, endpoints, names) where

import Data.Proxy (Proxy(..))
import Data.Type.Equality ((:~:)(Refl))
import GHC.TypeLits (Symbol, KnownSymbol, SomeSymbol(..), someSymbolVal, sameSymbol, symbolVal)

data Chain (a :: Symbol) (b :: Symbol) where
  Named :: String -> Chain a b
  Then :: Chain a b -> Chain b c -> Chain a c

named :: String -> Chain a b
named = Named

thenChain :: Chain a b -> Chain b c -> Chain a c
thenChain = Then

data SomeChain where
  SomeChain :: (KnownSymbol a, KnownSymbol b) => Proxy a -> Proxy b -> Chain a b -> SomeChain

atom :: String -> String -> String -> SomeChain
atom label source target = case (someSymbolVal source, someSymbolVal target) of
  (SomeSymbol a, SomeSymbol b) -> SomeChain a b (Named label)

joinChain :: SomeChain -> SomeChain -> Either String SomeChain
joinChain (SomeChain a b left) (SomeChain c d right) = case sameSymbol b c of
  Just Refl -> Right (SomeChain a d (Then left right))
  Nothing -> Left ("endpoint mismatch: " ++ symbolVal b ++ " /= " ++ symbolVal c)

endpoints :: SomeChain -> (String,String)
endpoints (SomeChain a b _) = (symbolVal a,symbolVal b)

names :: SomeChain -> [String]
names (SomeChain _ _ term) = go term where
  go :: Chain a b -> [String]
  go (Named label) = [label]
  go (Then left right) = go left ++ go right
