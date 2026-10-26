module Main (main) where
import Zal.Logic.Syntax

-- Exactly the same observation bodies imported by the population engine.
main :: IO ()
main = mapM_ test [(k,m,a,d,mode) | k<-[0..2],m<-[1..3],a<-[0..max k (m-1)],d<-[0..1],mode<-[0..2]] where
  test (k,m,a,d,mode) = do
    let obs=case mode of 0->Strict k; 1->Saturated k; _->Residue m
        valid=observe obs a==Just a
        result=do increment<-observe obs d; values<-addObserved [obs] [a] [increment]; case values of [r]->Just r; _->Nothing
        (reason,after)=if not valid then (200,a) else case result of Nothing->(201,a);Just r->(0,r)
    putStrLn (unwords (map show [k,m,a,d,mode,reason,after]))
