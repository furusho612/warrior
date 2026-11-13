---------------------------- MODULE Auth ---------------------------------------
(***************************************************************************)
(* 認証のチャレンジ、OAuth の状態、セッション、パスキーの状態遷移。       *)
(* 定数は小さくし、TLC で短時間に検査する。                               *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets

CONSTANTS Users, Challenges, OAuthStates, Sessions, Passkeys, GoogleUsers,
          BreakChallengeGuard, BreakOAuthGuard, BreakLogoutGuard,
          BreakPasskeyGuard

VARIABLES issuedChallenges, challengeResults, issuedOAuth, oauthResults,
          activeSessions, endedSessions, invalidUses, passkeys, google

vars == <<issuedChallenges, challengeResults, issuedOAuth, oauthResults,
          activeSessions, endedSessions, invalidUses, passkeys, google>>

TypeOK ==
  /\ issuedChallenges \subseteq Challenges
  /\ challengeResults \in [Challenges -> 0..2]
  /\ issuedOAuth \subseteq OAuthStates
  /\ oauthResults \in [OAuthStates -> 0..2]
  /\ activeSessions \subseteq Sessions
  /\ endedSessions \subseteq Sessions
  /\ invalidUses \subseteq Sessions
  /\ passkeys \in [Users -> SUBSET Passkeys]
  /\ google \subseteq Users

Init ==
  /\ issuedChallenges = {}
  /\ challengeResults = [c \in Challenges |-> 0]
  /\ issuedOAuth = {}
  /\ oauthResults = [o \in OAuthStates |-> 0]
  /\ activeSessions = {}
  /\ endedSessions = {}
  /\ invalidUses = {}
  /\ passkeys = [u \in Users |-> {}]
  /\ google = GoogleUsers

IssueChallenge(c) ==
  /\ c \notin issuedChallenges
  /\ issuedChallenges' = issuedChallenges \cup {c}
  /\ UNCHANGED <<challengeResults, issuedOAuth, oauthResults,
                 activeSessions, endedSessions, invalidUses, passkeys, google>>

ConsumeChallenge(c) ==
  /\ c \in issuedChallenges
  /\ (BreakChallengeGuard \/ challengeResults[c] = 0)
  /\ challengeResults' = [challengeResults EXCEPT ![c] = @ + 1]
  /\ UNCHANGED <<issuedChallenges, issuedOAuth, oauthResults,
                 activeSessions, endedSessions, invalidUses, passkeys, google>>

ExpireChallenge(c) ==
  /\ c \in issuedChallenges
  /\ challengeResults[c] = 0
  /\ issuedChallenges' = issuedChallenges \ {c}
  /\ UNCHANGED <<challengeResults, issuedOAuth, oauthResults,
                 activeSessions, endedSessions, invalidUses, passkeys, google>>

IssueOAuth(o) ==
  /\ o \notin issuedOAuth
  /\ issuedOAuth' = issuedOAuth \cup {o}
  /\ UNCHANGED <<issuedChallenges, challengeResults, oauthResults,
                 activeSessions, endedSessions, invalidUses, passkeys, google>>

ConsumeOAuth(o) ==
  /\ o \in issuedOAuth
  /\ (BreakOAuthGuard \/ oauthResults[o] = 0)
  /\ oauthResults' = [oauthResults EXCEPT ![o] = @ + 1]
  /\ UNCHANGED <<issuedChallenges, challengeResults, issuedOAuth,
                 activeSessions, endedSessions, invalidUses, passkeys, google>>

ExpireOAuth(o) ==
  /\ o \in issuedOAuth
  /\ oauthResults[o] = 0
  /\ issuedOAuth' = issuedOAuth \ {o}
  /\ UNCHANGED <<issuedChallenges, challengeResults, oauthResults,
                 activeSessions, endedSessions, invalidUses, passkeys, google>>

Login(u, s) ==
  /\ (u \in google \/ passkeys[u] # {})
  /\ s \notin activeSessions \cup endedSessions
  /\ activeSessions' = activeSessions \cup {s}
  /\ UNCHANGED <<issuedChallenges, challengeResults, issuedOAuth, oauthResults,
                 endedSessions, invalidUses, passkeys, google>>

Logout(s) ==
  /\ s \in activeSessions
  /\ activeSessions' = activeSessions \ {s}
  /\ endedSessions' = endedSessions \cup {s}
  /\ UNCHANGED <<issuedChallenges, challengeResults, issuedOAuth, oauthResults,
                 invalidUses, passkeys, google>>

UseSession(s) ==
  /\ (s \in activeSessions \/ (BreakLogoutGuard /\ s \in endedSessions))
  /\ invalidUses' = IF s \in endedSessions THEN invalidUses \cup {s} ELSE invalidUses
  /\ UNCHANGED <<issuedChallenges, challengeResults, issuedOAuth, oauthResults,
                 activeSessions, endedSessions, passkeys, google>>

AddPasskey(u, p) ==
  /\ p \notin passkeys[u]
  /\ passkeys' = [passkeys EXCEPT ![u] = @ \cup {p}]
  /\ UNCHANGED <<issuedChallenges, challengeResults, issuedOAuth, oauthResults,
                 activeSessions, endedSessions, invalidUses, google>>

DeletePasskey(u, p) ==
  /\ p \in passkeys[u]
  /\ passkeys' = [passkeys EXCEPT ![u] = @ \ {p}]
  (* 最後のパスキーを削除しても、Google アカウントでのログインは残る。 *)
  /\ google' = IF BreakPasskeyGuard THEN google \ {u} ELSE google
  /\ UNCHANGED <<issuedChallenges, challengeResults, issuedOAuth, oauthResults,
                 activeSessions, endedSessions, invalidUses>>

Next ==
  \/ \E c \in Challenges: IssueChallenge(c)
  \/ \E c \in Challenges: ConsumeChallenge(c)
  \/ \E c \in Challenges: ExpireChallenge(c)
  \/ \E o \in OAuthStates: IssueOAuth(o)
  \/ \E o \in OAuthStates: ConsumeOAuth(o)
  \/ \E o \in OAuthStates: ExpireOAuth(o)
  \/ \E u \in Users, s \in Sessions: Login(u, s)
  \/ \E s \in Sessions: Logout(s)
  \/ \E s \in Sessions: UseSession(s)
  \/ \E u \in Users, p \in Passkeys: AddPasskey(u, p)
  \/ \E u \in Users, p \in Passkeys: DeletePasskey(u, p)

Spec == Init /\ [][Next]_vars

ChallengeAtMostOnce ==
  \A c \in Challenges: challengeResults[c] <= 1

OAuthAtMostOnce ==
  \A o \in OAuthStates: oauthResults[o] <= 1

ExpiredChallengesRejected ==
  \A c \in Challenges: c \notin issuedChallenges => challengeResults[c] = 0

ExpiredOAuthRejected ==
  \A o \in OAuthStates: o \notin issuedOAuth => oauthResults[o] = 0

NoInvalidUse ==
  invalidUses = {}

SessionsAreNotReused ==
  activeSessions \cap endedSessions = {}

GoogleLoginPreserved ==
  google = GoogleUsers

Inv ==
  /\ TypeOK
  /\ ChallengeAtMostOnce
  /\ OAuthAtMostOnce
  /\ ExpiredChallengesRejected
  /\ ExpiredOAuthRejected
  /\ NoInvalidUse
  /\ SessionsAreNotReused
  /\ GoogleLoginPreserved

=============================================================================
