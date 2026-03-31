---
description: Batch review transfer pairs (16 at a time)
model: haiku
user_invocable: true
allowed-tools: Bash, AskUserQuestion
---

# Batch Transfer Review

Review pending transfer pairs in batches of 16, presenting each pair individually for confirmation.

## Workflow

### Step 1: Peek at 16 pairs

Run this command to see the next 16 pending pairs without making any changes (all skips):

```bash
printf 's\ns\ns\ns\ns\ns\ns\ns\ns\ns\ns\ns\ns\ns\ns\ns\n' | cargo run --bin transfers -- --review 16
```

Parse the output. Each pair looks like:

```
[N/16] $AMOUNT (DATE_A -> DATE_B) CONFIDENCE
  A: PAYEE_A                                  (acct: ACCOUNT_A)
  B: PAYEE_B                                  (acct: ACCOUNT_B)
  [y]es [n]o [s]kip [q]uit > s
```

Also note the status summary at the end (pending/confirmed/rejected counts).

If there are 0 pending pairs, tell the user and stop.

### Step 2: Present ALL 16 pairs to user

Use AskUserQuestion to present the pairs. You MUST present all 16 pairs before proceeding. Use 4 calls to AskUserQuestion with 4 questions each. Each question should have options: Yes, No, Skip.

Format each question as:

```
$AMOUNT (DATE_A → DATE_B) CONFIDENCE
A: PAYEE_A (ACCOUNT_A)
B: PAYEE_B (ACCOUNT_B)

Is this a transfer?
```

Use the pair number as the header (e.g. "Pair 1/16", "Pair 2/16", etc).

IMPORTANT: Collect ALL 16 answers before proceeding to Step 3. Do NOT apply any decisions until all answers are collected.

If the user answers "quit" to any question, stop the review immediately. Do not ask remaining questions or apply any decisions.

### Step 3: Apply decisions

Convert the 16 answers to y/n/s characters and pipe them into the binary:

```bash
printf 'y\nn\ns\ny\n...\n' | cargo run --bin transfers -- --review 16
```

Map: Yes→y, No→n, Skip→s.

Report the results summary from the cargo output (confirmed/rejected/skipped counts and total status).

### Step 4: Loop

Go back to Step 1 automatically. Continue until the user says quit or there are no more pending pairs.
