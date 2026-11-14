---------------------------- MODULE TrainingOwnership ----------------------------
(***************************************************************************)
(* 記録中のトレーニングの編集権、引き継ぎ、終了後の同時編集の TLA+ 仕様。 *)
(* 定数は端末 2 台、操作 2 つとし、TLC で短時間に検査する。               *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets

CONSTANTS Devices, Ops, BreakOwnerGuard, BreakHandoffGuard

VARIABLES status, editors, server, accepted, pending, handoffSource

vars == <<status, editors, server, accepted, pending, handoffSource>>

TypeOK ==
  /\ status \in {"recording", "finished"}
  /\ editors \subseteq Devices
  /\ server \in [Ops -> Devices \cup {"none"}]
  /\ accepted \subseteq Ops
  /\ pending \in [Devices -> SUBSET Ops]
  /\ handoffSource \subseteq Ops

(***************************************************************************)
(* 端末トークンを持たない移行前の記録中の行は所有者を持たない。初期状態に含む。 *)
(***************************************************************************)
Finished ==
  /\ status = "finished"
  /\ editors = Devices
  /\ server = [o \in Ops |-> "none"]
  /\ accepted = {}
  /\ pending = [d \in Devices |-> {}]
  /\ handoffSource = {}

LegacyRecording ==
  /\ status = "recording"
  /\ editors = {}
  /\ server = [o \in Ops |-> "none"]
  /\ accepted = {}
  /\ pending = [d \in Devices |-> {}]
  /\ handoffSource = {}

Init ==
  \/ Finished
  \/ LegacyRecording

Start(d) ==
  /\ status = "finished"
  /\ status' = "recording"
  /\ editors' = {d}
  /\ server' = [o \in Ops |-> "none"]
  /\ accepted' = {}
  /\ pending' = [e \in Devices |-> {}]
  /\ handoffSource' = {}

Write(d, o) ==
  /\ status = "recording"
  /\ d \in editors
  /\ o \notin pending[d]
  /\ pending' = [pending EXCEPT ![d] = @ \cup {o}]
  /\ UNCHANGED <<status, editors, server, accepted, handoffSource>>

(***************************************************************************)
(* 後勝ちは、同じ操作への Sync と EditAfterFinish が server を順に上書き  *)
(* することで表す。server は各操作について最後に受理した端末を保持する。 *)
(***************************************************************************)
Sync(d, o) ==
  /\ status = "recording"
  /\ d \in editors
  /\ o \in pending[d]
  /\ server' = [server EXCEPT ![o] = d]
  /\ accepted' = accepted \cup {o}
  /\ pending' = [pending EXCEPT ![d] = @ \ {o}]
  /\ UNCHANGED <<status, editors, handoffSource>>

Handoff(d) ==
  /\ status = "recording"
  /\ d \in Devices
  /\ editors' = IF BreakOwnerGuard THEN editors \cup {d} ELSE {d}
  /\ handoffSource' =
       IF BreakHandoffGuard
       THEN accepted \cup (UNION {pending[e] : e \in editors})
       ELSE accepted
  /\ UNCHANGED <<status, server, accepted, pending>>

(***************************************************************************)
(* 所有者の無い記録中の行は、最初に書き込んだ端末が所有者になる (migration)。 *)
(***************************************************************************)
Claim(d) ==
  /\ status = "recording"
  /\ editors = {}
  /\ editors' = {d}
  /\ UNCHANGED <<status, server, accepted, pending,
                 handoffSource>>

Finish ==
  /\ status = "recording"
  /\ status' = "finished"
  /\ editors' = Devices
  /\ UNCHANGED <<server, accepted, pending, handoffSource>>

EditAfterFinish(d, o) ==
  /\ status = "finished"
  /\ server' = [server EXCEPT ![o] = d]
  /\ accepted' = accepted \cup {o}
  /\ UNCHANGED <<status, editors, pending, handoffSource>>

Next ==
  \/ \E d \in Devices: Start(d)
  \/ \E d \in Devices, o \in Ops: Write(d, o)
  \/ \E d \in Devices, o \in Ops: Sync(d, o)
  \/ \E d \in Devices: Handoff(d)
  \/ \E d \in Devices: Claim(d)
  \/ Finish
  \/ \E d \in Devices, o \in Ops: EditAfterFinish(d, o)

Spec == Init /\ [][Next]_vars

AtMostOneEditor ==
  status = "recording" => Cardinality(editors) <= 1

HandoffFromSaved ==
  handoffSource \subseteq accepted

AfterFinishAllCanEdit ==
  status = "finished" => editors = Devices

Inv ==
  /\ TypeOK
  /\ AtMostOneEditor
  /\ HandoffFromSaved
  /\ AfterFinishAllCanEdit

=============================================================================
