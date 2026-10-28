{-# LANGUAGE OverloadedStrings #-}
module Main (main) where

import Control.Monad (foldM, unless)
import Data.Aeson
import Data.Aeson.Types (Parser, parseEither)
import qualified Data.Aeson.KeyMap as KM
import qualified Data.ByteString as B
import qualified Data.ByteString.Lazy.Char8 as L
import qualified Data.Map.Strict as M
import qualified Data.Set as S
import System.IO (stdin)
import Zal.Lens.FiniteCategory
import Zal.Lens.Typed (atom, joinChain, names, endpoints)

keys :: [Key] -> Object -> Parser ()
keys expected o = unless (S.fromList expected == S.fromList (KM.keys o)) (fail "unknown or missing fields")

pair :: Value -> Parser (String,String)
pair = withArray "pair" $ \a -> case foldr (:) [] a of
  [x,y] -> (,) <$> parseJSON x <*> parseJSON y
  _ -> fail "mapping entry must be a pair"

parseCategory :: Value -> Parser RawCategory
parseCategory = withObject "category" $ \o -> do
  keys ["id","objects","arrows","identities","composition"] o
  as <- o .: "arrows" >>= mapM (withObject "arrow" $ \a -> do
    keys ["id","source","target"] a
    RawArrow <$> a .: "id" <*> a .: "source" <*> a .: "target")
  ids <- o .: "identities" >>= mapM pair
  cs <- o .: "composition" >>= mapM (withArray "composition triple" $ \a -> case foldr (:) [] a of
    [f,g,h] -> do x <- parseJSON f; y <- parseJSON g; z <- parseJSON h; pure ((x,y),z)
    _ -> fail "composition must be [before,after,result]")
  RawCategory <$> o .: "id" <*> o .: "objects" <*> pure as <*> pure ids <*> pure cs

declarations :: Object -> Parser ()
declarations o = mapM_ (\k -> o .: k >>= (parseJSON :: Value -> Parser [String]) >> pure ()) ["assumptions","preserves","forgets"]

parseFunctor :: Value -> Parser (RawFunctor,Value)
parseFunctor value = withObject "functor" (\o -> do
  keys ["id","source","target","objects","arrows","assumptions","preserves","forgets"] o
  declarations o
  os <- o .: "objects" >>= mapM pair; as <- o .: "arrows" >>= mapM pair
  f <- RawFunctor <$> o .: "id" <*> o .: "source" <*> o .: "target" <*> pure os <*> pure as
  pure (f,value)) value

data Relationship = Relationship String String String Value
parseRelationship :: Value -> Parser Relationship
parseRelationship value = withObject "relationship" (\o -> do
  keys ["id","source","target","source_key","target_key","assumptions","preserves","forgets"] o
  declarations o
  Relationship <$> o .: "id" <*> o .: "source_key" <*> o .: "target_key" <*> pure value) value

data Request = Request String [RawCategory] [(RawFunctor,Value)] [Relationship] Value Int
request :: Value -> Parser Request
request = withObject "request" $ \o -> do
  keys ["schema","operation","source_context","categories","functors","relationships","selection","budget"] o
  schema <- o .: "schema" :: Parser String
  unless (schema == "zal/lens-category/1") (fail "unsupported schema")
  budget <- o .: "budget" :: Parser Int
  unless (budget >= 0 && budget <= 1000000) (fail "budget outside 0..1000000")
  cs <- o .: "categories" >>= mapM parseCategory
  fs <- o .: "functors" >>= mapM parseFunctor
  rs <- o .: "relationships" >>= mapM parseRelationship
  unless (length cs <= 4 && length fs <= 16 && length rs <= 32) (fail "declaration bounds exceeded")
  Request <$> o .: "operation" <*> pure cs <*> pure fs <*> pure rs <*> o .: "selection" <*> pure budget

field :: FromJSON a => Key -> Value -> Check a
field key value = case parseEither (withObject "selection" (.: key)) value of
  Left err -> invalid err
  Right parsed -> pure parsed

selectionKeys :: [Key] -> Value -> Check ()
selectionKeys wanted value = case parseEither (withObject "selection" (keys wanted)) value of
  Left err -> invalid err
  Right () -> pure ()

get :: Ord k => String -> k -> M.Map k a -> Check a
get message key table = maybe (invalid message) pure (M.lookup key table)

functorValue :: FunctorMap -> Value
functorValue checked = let f = functorData checked in object
  ["id" .= rawFunctorId f,"source" .= fromCategory f,"target" .= toCategory f
  ,"objects" .= map (\(a,b) -> [a,b]) (objectMap f),"arrows" .= map (\(a,b) -> [a,b]) (arrowMap f)
  ,"object_fibers" .= fibers (objectMap f),"arrow_fibers" .= fibers (arrowMap f)]

losses :: [Value] -> Value
losses hops = toJSON (concatMap declared hops) where
  declared value = case parseEither (withObject "hop" (.: "forgets")) value of
    Right xs -> xs :: [String]
    Left _ -> [] -- Only called after strict hop declaration parsing.

run :: Request -> Either (Fault,Int) (Value,Int)
run (Request op rawCategories rawFunctors relationships selected cap) = runCheck cap $ do
  charge
  unless (unique (map rawCategoryId rawCategories) && unique (map (rawFunctorId . fst) rawFunctors)
          && unique [i | Relationship i _ _ _ <- relationships]) (invalid "duplicate declaration identity")
  cats <- mapM checkCategory rawCategories
  let cm = M.fromList [(categoryId c,c) | c <- cats]
  fs <- mapM (checkFunctor cm . fst) rawFunctors
  let fm = M.fromList [(rawFunctorId (functorData f),f) | f <- fs]
      declared = M.fromList [(rawFunctorId f,v) | (f,v) <- rawFunctors]
      selectedCategory = do i <- field "category" selected; get "unknown category" i cm
      path = do ps <- field "path" selected; unless (not (null ps) && length ps <= 32) (invalid "path length outside 1..32"); pure ps
  case op of
    "check-category" -> do
      selectionKeys ["category"] selected
      c <- selectedCategory
      pure (object ["claim" .= ("finite-category-laws"::String),"category" .= categoryId c
                   ,"objects" .= length (categoryObjects c),"arrows" .= length (categoryArrows c)])
    "compose-path" -> do
      selectionKeys ["category","path"] selected
      c <- selectedCategory; ps <- path; result <- pathComposite c ps
      pure (object ["claim" .= ("declared-category-path-composite"::String),"path" .= ps,"composite" .= result])
    "invertible" -> do
      selectionKeys ["category","arrow"] selected
      c <- selectedCategory; a <- field "arrow" selected; result <- inverse c a
      pure (object ["claim" .= ("two-sided-inverse-in-declared-category"::String),"arrow" .= (a::String),"inverse" .= result])
    "check-functor" -> do
      selectionKeys ["functor"] selected
      i <- field "functor" selected; f <- get "unknown functor" i fm; hop <- get "missing declaration" i declared
      pure (object ["claim" .= ("finite-functor-laws"::String),"functor" .= functorValue f
                   ,"hop_declarations" .= [hop],"cumulative_forgets" .= losses [hop]])
    "compose-functors" -> do
      selectionKeys ["path"] selected
      ps <- path; chosen <- mapM (\i -> get "unknown functor" i fm) ps
      f <- composeFunctors cm chosen; hops <- mapM (\i -> get "missing declaration" i declared) ps
      pure (object ["claim" .= ("finite-composite-functor-laws"::String),"functor" .= functorValue f
                   ,"hop_declarations" .= hops,"cumulative_forgets" .= losses hops])
    "compose-symbolic" -> do
      selectionKeys ["path"] selected
      ps <- path
      let rm = M.fromList [(i,(s,t,v)) | Relationship i s t v <- relationships]
      hops <- mapM (\i -> do (s,t,v) <- get "unknown relationship" i rm; pure (i,s,t,v)) ps
      case hops of
        [] -> invalid "empty chain"
        ((i,s,t,v):rest) -> do
          (chain,values) <- foldM (\(prior,vs) (j,a,b,w) -> do
            charge
            case joinChain prior (atom j a b) of
              Left message -> refute "typed-context-composition" [("detail",message)]
              Right next -> pure (next,vs ++ [w])) (atom i s t,[v]) rest
          pure (object ["claim" .= ("authored-symbolic-chain-only"::String),"path" .= names chain
                       ,"endpoints" .= endpoints chain,"hop_declarations" .= values,"cumulative_forgets" .= losses values])
    _ -> invalid "unsupported operation"
  where unique xs = length xs == S.size (S.fromList xs)

response :: Request -> Value
response req@(Request op _ _ _ _ _) = case run req of
  Right (result,used) -> object ["status" .= (if op == "compose-symbolic" then "Authored" else "CheckedAt" :: String)
                              ,"work_used" .= used,"result" .= result]
  Left (fault,used) -> case fault of
    Invalid message -> object ["status" .= ("Invalid"::String),"work_used" .= used,"message" .= message]
    Unknown message -> object ["status" .= ("UNKNOWN"::String),"work_used" .= used,"message" .= message]
    Refuted law witness -> object ["status" .= ("RefutedAt"::String),"work_used" .= used,"law" .= law,"witness" .= witness]

-- One bounded request, no imported code, no loop or persistent state. The public
-- captured Python ingress rejects duplicate JSON keys before aeson collapse.
main :: IO ()
main = do
  raw <- B.hGet stdin (1024*1024+1)
  let result = if B.length raw > 1024*1024
        then object ["status" .= ("UNKNOWN"::String),"work_used" .= (0::Int),"message" .= ("input byte bound"::String)]
        else case eitherDecodeStrict' raw >>= parseEither request of
          Left message -> object ["status" .= ("Invalid"::String),"work_used" .= (0::Int),"message" .= message]
          Right req -> response req
  L.putStrLn (encode result)
