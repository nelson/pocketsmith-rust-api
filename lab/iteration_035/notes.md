# Iteration 035: Human name identification and canonicalisation

## Hypothesis
Identify all human names in transfer payees and add them to stage 4 person
rules. This includes: name variant consolidation (first name → full name),
memo stripping, title prefix removal (Mr/Mrs/Miss/Ms), reversed name
ordering, casing fixes, and couple variant consolidation. Also identify
business entities that were incorrectly appearing as person names.

## Changes
- **stage4_rules.yaml**: Added ~65 person variant mappings, ~25 memo-strip
  entries, ~10 business entity patterns (AES Electrical, Tan Hands Physio,
  Genesis Gardens, CS Education, etc.)
- **stage4_identity.py**: Enhanced `_canonicalise_person` to strip Mr/Mrs/Miss/Ms
  title prefixes and retry persons lookup; made persons_strip_memo recursive
  so stripped names also go through persons dict

## Key consolidations
- First name → full name: Dennis→Dennis Law, Gillian→Gillian Li, Zoe→Zoe Fan, etc.
- Reversed names: Lam Harry→Harry Lam, Davies K→Kirsten Davies, etc.
- Title stripping: Mr Andy Chi-Kit Tan→Andy Tan, Ms Doris Ming Wai Cho→Doris Chong
- Couple variants: Simon and Stephanie Wong→Stephanie and Simon Wong
- Memo stripping: 25+ names with Paint and Sip/event memos

## Result
Q: 93.59 → 93.66 (+0.07)
Unique payees: 4215 → 4168 (-47)
467 transactions changed
