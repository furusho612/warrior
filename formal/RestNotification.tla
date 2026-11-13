---------------------------- MODULE RestNotification ---------------------------
(***************************************************************************)
(* レスト終了のプッシュ通知の予約、取り消し、再予約、古いメッセージ、     *)
(* 終了後の送信の TLA+ 仕様。定数は小さくし、TLC で検査する。             *)
(***************************************************************************)

EXTENDS Naturals, FiniteSets

CONSTANTS Devices, Reservations, Messages, Times,
          BreakDuplicateGuard, BreakCancelGuard, BreakFinishGuard,
          BreakOldGuard, BreakExpiryGuard

VARIABLES owner, currentEnd, cancelled, finished, messageReservation,
          messageEnd, expired, sent, sentWhen, invalidSends

vars == <<owner, currentEnd, cancelled, finished, messageReservation,
          messageEnd, expired, sent, sentWhen, invalidSends>>

TypeOK ==
  /\ owner \in [Reservations -> Devices]
  /\ currentEnd \in [Reservations -> Times]
  /\ cancelled \subseteq Reservations
  /\ finished \subseteq Reservations
  /\ messageReservation \in [Messages -> Reservations]
  /\ messageEnd \in [Messages -> Times]
  /\ expired \subseteq Messages
  /\ sent \in [Messages -> 0..2]
  /\ sentWhen \in [Messages -> Times]
  /\ invalidSends \subseteq Messages

Init ==
  /\ owner = [r \in Reservations |-> CHOOSE d \in Devices: TRUE]
  /\ currentEnd = [r \in Reservations |-> CHOOSE t \in Times: TRUE]
  /\ cancelled = {}
  /\ finished = {}
  /\ messageReservation = [m \in Messages |-> CHOOSE r \in Reservations: TRUE]
  /\ messageEnd = [m \in Messages |-> CHOOSE t \in Times: TRUE]
  /\ expired = {}
  /\ sent = [m \in Messages |-> 0]
  /\ sentWhen = [m \in Messages |-> CHOOSE t \in Times: TRUE]
  /\ invalidSends = {}

Reserve(r, d, t) ==
  /\ owner' = [owner EXCEPT ![r] = d]
  /\ currentEnd' = [currentEnd EXCEPT ![r] = t]
  /\ cancelled' = cancelled \ {r}
  /\ finished' = finished \ {r}
  /\ UNCHANGED <<messageReservation, messageEnd, expired, sent, sentWhen,
                 invalidSends>>

Cancel(r) ==
  /\ cancelled' = cancelled \cup {r}
  /\ UNCHANGED <<owner, currentEnd, finished, messageReservation,
                 messageEnd, expired, sent, sentWhen, invalidSends>>

Reschedule(r, t) ==
  /\ currentEnd' = [currentEnd EXCEPT ![r] = t]
  /\ cancelled' = cancelled \ {r}
  /\ UNCHANGED <<owner, finished, messageReservation, messageEnd,
                 expired, sent, sentWhen, invalidSends>>

Finish(r) ==
  /\ finished' = finished \cup {r}
  /\ UNCHANGED <<owner, currentEnd, cancelled, messageReservation,
                 messageEnd, expired, sent, sentWhen, invalidSends>>

Expire(m) ==
  /\ expired' = expired \cup {m}
  /\ UNCHANGED <<owner, currentEnd, cancelled, finished,
                 messageReservation, messageEnd, sent, sentWhen, invalidSends>>

Send(m) ==
  /\ (BreakDuplicateGuard \/ sent[m] = 0)
  /\ (BreakCancelGuard \/ messageReservation[m] \notin cancelled)
  /\ (BreakFinishGuard \/ messageReservation[m] \notin finished)
  /\ (BreakExpiryGuard \/ m \notin expired)
  /\ (BreakOldGuard \/
        messageEnd[m] = currentEnd[messageReservation[m]])
  /\ sent' = [sent EXCEPT ![m] = @ + 1]
  /\ sentWhen' = [sentWhen EXCEPT ![m] =
        currentEnd[messageReservation[m]]]
  /\ invalidSends' =
       IF messageReservation[m] \in cancelled
          \/ messageReservation[m] \in finished
          \/ m \in expired
          \/ messageEnd[m] # currentEnd[messageReservation[m]]
       THEN invalidSends \cup {m}
       ELSE invalidSends
  /\ UNCHANGED <<owner, currentEnd, cancelled, finished,
                 messageReservation, messageEnd, expired>>

Next ==
  \/ \E r \in Reservations, d \in Devices, t \in Times: Reserve(r, d, t)
  \/ \E r \in Reservations: Cancel(r)
  \/ \E r \in Reservations, t \in Times: Reschedule(r, t)
  \/ \E r \in Reservations: Finish(r)
  \/ \E m \in Messages: Expire(m)
  \/ \E m \in Messages: Send(m)

Spec == Init /\ [][Next]_vars

AtMostOneNotification ==
  \A m \in Messages: sent[m] <= 1

NoInvalidSend ==
  invalidSends = {}

LatestEndUsed ==
  \A m \in Messages:
    sent[m] > 0 => sentWhen[m] = messageEnd[m]

Inv ==
  /\ TypeOK
  /\ AtMostOneNotification
  /\ NoInvalidSend
  /\ LatestEndUsed

=============================================================================
