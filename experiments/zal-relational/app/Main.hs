{-# LANGUAGE OverloadedStrings #-}
module Main (main) where

import Control.Monad (foldM, unless)
import Data.Aeson
import Data.Aeson.Types (Parser, parseEither)
import qualified Data.Aeson.Key as K
import qualified Data.Aeson.KeyMap as KM
import qualified Data.ByteString.Lazy.Char8 as B
import qualified Data.ByteString.Char8 as StrictB
import qualified Data.Map.Strict as M
import qualified Data.Set as S
import qualified Data.Text as T
import Data.Ratio ((%), numerator, denominator)
import System.IO (isEOF)
import Text.Read (readMaybe)
import Zal.Logic.Syntax
import Zal.Logic.Normalize
import Zal.Logic.Finite
import Zal.Logic.Population
import Zal.Logic.Expansion
import Zal.Logic.Budget (Stats(..))
import Zal.Logic.Compare

keys :: [K.Key] -> Object -> Parser ()
keys wanted o = unless (S.fromList wanted == S.fromList (KM.keys o)) (fail "unknown or missing fields")
natural :: Value -> Parser Integer
natural = withText "canonical natural string" $ \s ->
  let str=T.unpack s in if T.length s>256 || not (str=="0" || case str of c:cs->c>='1' && c<='9' && all (\a->a>='0' && a<='9') cs; []->False)
    then fail "noncanonical natural" else maybe (fail "natural parse failed") pure (readMaybe str)
rational :: Value -> Parser Rational
rational = withText "canonical rational string" $ \s -> do
  unless (T.length s<=256) (fail "rational too long")
  r<-case T.splitOn "/" s of
    [a] -> maybe (fail "invalid rational") (pure . fromInteger) (readMaybe (T.unpack a))
    [a,b] -> case (readMaybe (T.unpack a),readMaybe (T.unpack b)) of
      (Just n,Just d) | d>0 -> pure (n%d)
      _ -> fail "invalid rational denominator"
    _ -> fail "invalid rational"
  unless (renderRational r==s) (fail "rational must be reduced and canonical")
  pure r
renderRational :: Rational -> T.Text
renderRational r = T.pack (show (numerator r) ++ if denominator r==1 then "" else "/"++show (denominator r))
variable :: Value -> Parser Var
variable = withText "variable" $ \s -> case s of "x"->pure X; "y"->pure Y; _->fail "only x and y are supported; never alias a third variable"
comparison :: Value -> Parser Cmp
comparison = withText "comparison" $ \s -> case s of "eq"->pure CEq; "le"->pure CLe; "ge"->pure CGe; _->fail "unknown comparison"
formula :: Value -> Parser Formula
formula = withObject "formula" $ \o -> do
  op<-o .: "op" :: Parser T.Text
  let body= o .: "body" >>= formula
      two constructor=keys ["op","left","right"] o >> (constructor <$> (o .: "left" >>= formula) <*> (o .: "right" >>= formula))
      bind constructor=keys ["op","var","body"] o >> (constructor <$> (o .: "var" >>= variable) <*> body)
  case op of
    "true"->keys ["op"] o >> pure (Truth True)
    "false"->keys ["op"] o >> pure (Truth False)
    "atom"->do
      keys ["op","predicate","args"] o
      args<-o .: "args" >>= mapM variable
      Atom <$> o .: "predicate" <*> pure args
    "equal"->do
      keys ["op","args"] o
      vs<-o .: "args" >>= mapM variable
      case vs of [a,b]->pure (Equal a b); _->fail "equality arity"
    "not"->keys ["op","body"] o >> (Not <$> body)
    "and"->two And
    "or"->two Or
    "implies"->two Implies
    "iff"->two Iff
    "forall"->bind Forall
    "exists"->bind Exists
    "count"->do
      keys ["op","var","cmp","threshold","body"] o
      Count <$> (o .: "var" >>= variable) <*> (Ordinary <$> (o .: "cmp" >>= comparison) <*> (o .: "threshold" >>= natural)) <*> body
    "mod_count"->do
      keys ["op","var","cmp","residue","modulus","body"] o
      Count <$> (o .: "var" >>= variable) <*> (Modular <$> (o .: "cmp" >>= comparison) <*> (o .: "residue" >>= natural) <*> (o .: "modulus" >>= natural)) <*> body
    _ -> fail "unsupported formula constructor"

query :: Value -> Parser Query
query = withObject "query" $ \o -> do
  keys ["schema","logic","signature","formula","operation","domain_size","weights","context_id"] o
  schema<-o .: "schema"; operation<-o .: "operation"
  unless (schema==("zal/relational-query/1"::T.Text) && operation==("count"::T.Text)) (fail "unsupported query schema/operation")
  ctx<-o .: "context_id" :: Parser T.Text
  unless (not (T.null ctx) && T.length ctx<=256) (fail "invalid context reference")
  profileName<-o .: "logic" :: Parser T.Text
  p<-case profileName of "zal/c2/1"->pure C2; "zal/c2mod/1"->pure C2mod; _->fail "unsupported logic profile"
  sig<-o .: "signature" >>= mapM declaration
  unless (length sig==M.size (M.fromList sig)) (fail "duplicate predicate declaration")
  ws<-o .: "weights" >>= withObject "weights" (mapM weight . KM.toList)
  Query p (M.fromList sig) <$> (o .: "formula" >>= formula) <*> (o .: "domain_size" >>= natural) <*> pure (M.fromList ws)
  where
    declaration = withObject "predicate" $ \o -> do
      keys ["id","arity"] o
      p<-o .: "id"; ar<-o .: "arity" :: Parser Integer
      a<-case ar of 1->pure Unary; 2->pure Binary; _->fail "only unary/binary predicates"
      pure (p,a)
    weight (k,v) = do
      vals<-parseJSON v >>= mapM rational
      case vals of [a,b]->pure (K.toString k,Weight a b); _->fail "weight pair is [true,false]"

limits :: Value -> Parser Limits
limits = withObject "limits" $ \o -> do
  keys ["entries","candidates","work","bits","domain"] o
  Limits <$> (o .: "entries" >>= natural) <*> (o .: "candidates" >>= natural)
         <*> (o .: "work" >>= natural) <*> (o .: "bits" >>= natural) <*> (o .: "domain" >>= natural)

data Request
  = Request !Query !Limits !T.Text !(Maybe Value)
  | EvaluateRequest !Query !Limits !Value
  | CompareRequest !Query !Query !Query !Limits !Relation

-- Public model interpretation/comparison has no weights: it talks about truth.
-- Reuse exactly the same query parser and typed validator as the counting port.
unweightedQuery :: Object -> Value -> Parser Query
unweightedQuery o f = do
  common<-mapM (\k -> (k .=) <$> (o .: k :: Parser Value)) ["logic","signature","domain_size","context_id"]
  sig<-o .: "signature" :: Parser [Value]
  names<-mapM (withObject "predicate" (.: "id")) sig :: Parser [T.Text]
  query (object (common ++ ["schema" .= ("zal/relational-query/1"::T.Text),"operation" .= ("count"::T.Text)
    ,"formula" .= f,"weights" .= object [K.fromText p .= (["1","1"]::[T.Text]) | p<-names]]))

schemaIs :: T.Text -> Object -> Parser ()
schemaIs wanted o = do
  actual<-o .: "schema"
  unless (actual==wanted) (fail "unsupported public request schema")

publicEvaluation :: Limits -> Value -> Parser Request
publicEvaluation lim = withObject "evaluation request" $ \o -> do
  keys ["schema","logic","signature","formula","domain_size","context_id","model"] o
  schemaIs "zal/relational-evaluation/1" o
  q<-o .: "formula" >>= unweightedQuery o
  EvaluateRequest q lim <$> o .: "model"

publicComparison :: Limits -> Value -> Parser Request
publicComparison lim = withObject "comparison request" $ \o -> do
  keys ["schema","logic","signature","domain_size","context_id","assumptions","left","right","relation"] o
  schemaIs "zal/relational-comparison/1" o
  named<-o .: "relation" :: Parser T.Text
  rel<-case named of "equivalence"->pure Equivalence; "implication"->pure Implication; _->fail "unsupported relation"
  a<-o .: "assumptions" >>= unweightedQuery o
  l<-o .: "left" >>= unweightedQuery o
  r<-o .: "right" >>= unweightedQuery o
  pure (CompareRequest a l r lim rel)

request :: Value -> Parser Request
request = withObject "internal request" $ \o -> do
  mode<-o .: "mode" :: Parser T.Text
  if mode=="compare" || mode=="evaluate-public" then do
    keys ["request","limits","mode"] o
    lim<-o .: "limits" >>= limits
    val<-o .: "request"
    if mode=="compare" then publicComparison lim val else publicEvaluation lim val
  else do
    keys ["query","limits","mode","model"] o
    Request <$> (o .: "query" >>= query) <*> (o .: "limits" >>= limits) <*> pure mode <*> o .: "model"

failure :: Failure -> Value
failure f = case f of
  Invalid s -> simple "Invalid" s
  Unsupported s -> simple "Unsupported" s
  InternalDisagreement s -> simple "InternalDisagreement" s
  ResourceExhausted resource used cap -> object ["status" .= ("ResourceExhausted"::T.Text),"resource" .= resource,"used" .= show used,"limit" .= show cap]
  where simple status message=object ["status" .= (status::T.Text),"message" .= message]

runRequest :: Request -> Value
runRequest (EvaluateRequest q lim val) = either failure (\(b,bound) -> object
  ["status" .= ("Evaluated"::T.Text),"value" .= b,"domain_size" .= show (domainSize q)
  ,"work_bound" .= show bound,"engine" .= ("original-formula-evaluation/1"::T.Text)]) $ do
  checked<-checkQuery lim q
  m<-parseModel checked val
  evaluateBounded lim checked m
runRequest (CompareRequest a l r lim rel) = either failure (renderComparison rel (domainSize a)) $ do
  ca<-checkQuery lim a
  cl<-checkQuery lim l
  cr<-checkQuery lim r
  compareAt lim rel ca cl cr
runRequest (Request q lim mode suppliedModel) = either failure run (checkQuery lim q) where
  run c = case mode of
    "count" -> either failure (\(r,s) -> object
      ["status" .= ("ExactCount"::T.Text),"value" .= renderRational r,"domain_size" .= show (domainSize q)
      ,"engine" .= ("population-definitions/1"::T.Text),"work" .= show (work s)
      ,"peak_entries" .= show (peakEntries s),"peak_bits" .= show (peakBits s)
      ,"assurance" .= ("experimental; source-level proof open"::T.Text)]) (countModels lim c)
    "enumerate" -> either failure (\r->object ["status" .= ("ExactCount"::T.Text),"value" .= renderRational r]) $ do
      ms<-enumerateStructures lim c
      foldM (\total m -> do ok<-evaluate c m; pure (total+if ok then weightOf c m else 0)) 0 ms
    "evaluate" -> either failure (\b->object ["status" .= ("Evaluated"::T.Text),"value" .= b]) $ do
      m<-modelFor c
      evaluate c m
    "expansion" -> either failure (\(n,b)->object ["status" .= ("ExpansionCount"::T.Text),"expansions" .= show n,"original" .= b]) $ do
      m<-modelFor c
      expansionsAt lim c m
    "normalize" -> let n=normalize c in object
      ["status" .= ("Prepared"::T.Text),"root" .= show (root n)
      ,"row_count" .= length (rows n),"global_count" .= length (globals n)
      ,"steps" .= [object ["id" .= show (definitionId d),"source" .= show (definitionSource d)
                          ,"relation" .= definitionRelation d,"role" .= definitionRole d] | d<-definitions n]
      ,"body_predicates" .= ("deterministic definitions inlined, unit auxiliary weights"::T.Text)]
    _ -> failure (Unsupported "unknown internal operation")
  modelFor c = maybe (Left (Invalid "missing complete model")) (parseModel c) suppliedModel

parseModel :: Checked -> Value -> Either Failure Structure
parseModel c val = do
  tables<-either (Left . Invalid) Right (parseEither (withObject "model" (mapM table . KM.toList)) val)
  structure c (M.fromList tables)
  where
    table (p,v) = do
      tuples<-parseJSON v >>= mapM (mapM natural)
      unless (length tuples==S.size (S.fromList tuples)) (fail "duplicate model tuple")
      pure (K.toString p,S.fromList tuples)

renderComparison :: Relation -> Integer -> Comparison -> Value
renderComparison rel n result = object
  ["status" .= status,"holds" .= holds,"vacuous" .= vacuous
  ,"relation" .= relationName,"domain_size" .= show n
  ,"enumerated_models" .= show (enumerated result),"admitted_models" .= show (admitted result)
  ,"partition" .= object ["both_true" .= show (bothTrue h),"left_only" .= show (leftOnly h)
                         ,"right_only" .= show (rightOnly h),"both_false" .= show (bothFalse h)]
  ,"counterexample" .= counterexample,"work_bound" .= show (workBound result)
  ,"engine" .= ("complete-structure-comparison/1"::T.Text)
  ,"scope" .= ("all labeled structures at this domain and signature; under the supplied assumptions"::T.Text)]
  where
    h=partition result
    vacuous=admitted result==0
    failed=case witness result of Nothing->False; Just _->True
    relationName=if rel==Equivalence then "equivalence" else "implication" :: T.Text
    status | vacuous = "VacuousAt"
           | failed = "RefutedAt"
           | rel==Equivalence = "EquivalentAt"
           | otherwise = "ImpliesAt" :: T.Text
    holds=if vacuous then Nothing else Just (not failed)
    counterexample=case witness result of
      Nothing -> Null
      Just (m,a,b) -> object
        ["model" .= object [K.fromString p .= map (map show) (S.toList ts) | (p,ts)<-M.toList (structureRelations m)]
        ,"assumptions" .= True,"left" .= a,"right" .= b]

-- Internal worker only. The supported ingress is run.py, which bounds raw
-- bytes/depth and rejects duplicate JSON keys before aeson can collapse them.
main :: IO ()
main = do
  end<-isEOF
  unless end $ do
    line<-B.fromStrict <$> StrictB.getLine
    let result=case eitherDecode line >>= parseEither request of
          Left err -> failure (Invalid err)
          Right req -> runRequest req
    B.putStrLn (encode result)
    main
