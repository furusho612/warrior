---------------------------- MODULE OfflineSync --------------------------------
(***************************************************************************)
(* 端末内の送信待ちの列と、サーバーへの適用の TLA+ 仕様。                 *)
(* 定数は端末 2 台、操作 2 つとし、TLC で短時間に検査する。               *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets

CONSTANTS Devices, Ops, BreakDuplicateGuard

VARIABLES queue, local, accepted, counts, server

vars == <<queue, local, accepted, counts, server>>

TypeOK ==
  /\ queue \in [Devices -> SUBSET Ops]
  /\ local \in [Devices -> SUBSET Ops]
  /\ accepted \subseteq Ops
  /\ counts \in [Ops -> 0..2]
  /\ server \in [Ops -> Devices \cup {"none"}]

Init ==
  /\ queue = [d \in Devices |-> {}]
  /\ local = [d \in Devices |-> {}]
  /\ accepted = {}
  /\ counts = [o \in Ops |-> 0]
  /\ server = [o \in Ops |-> "none"]

Enqueue(d, o) ==
  /\ o \notin queue[d]
  /\ queue' = [queue EXCEPT ![d] = @ \cup {o}]
  /\ local' = [local EXCEPT ![d] = @ \cup {o}]
  /\ UNCHANGED <<accepted, counts, server>>

Send(d, o) ==
  /\ o \in queue[d]
  /\ (BreakDuplicateGuard \/ o \notin accepted)
  /\ accepted' = accepted \cup {o}
  /\ counts' = [counts EXCEPT ![o] = @ + 1]
  /\ server' = [server EXCEPT ![o] = d]
  /\ UNCHANGED <<queue, local>>

Ack(d, o) ==
  /\ o \in queue[d]
  /\ o \in accepted
  /\ queue' = [queue EXCEPT ![d] = @ \ {o}]
  /\ UNCHANGED <<local, accepted, counts, server>>

Retry(d, o) ==
  /\ o \in queue[d]
  /\ UNCHANGED vars

Next ==
  \/ \E d \in Devices, o \in Ops: Enqueue(d, o)
  \/ \E d \in Devices, o \in Ops: Send(d, o)
  \/ \E d \in Devices, o \in Ops: Ack(d, o)
  \/ \E d \in Devices, o \in Ops: Retry(d, o)

Spec == Init /\ [][Next]_vars

NoDuplicateSets ==
  \A o \in Ops: counts[o] <= 1

AcceptedApplied ==
  \A o \in Ops: o \in accepted => server[o] # "none"

QueuedIsLocal ==
  \A d \in Devices: queue[d] \subseteq local[d]

UnsyncedNotOverwritten ==
  \A d \in Devices: queue[d] \subseteq local[d]

Inv ==
  /\ TypeOK
  /\ NoDuplicateSets
  /\ AcceptedApplied
  /\ QueuedIsLocal
  /\ UnsyncedNotOverwritten

=============================================================================
