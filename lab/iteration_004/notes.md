# Iteration 004

## Hypothesis
Adding stage1 suffix patterns for EFTPOS receipts, and a prefix for "Visa Debit Purchase Card XXXX" will collapse ~100 variants that have unique receipt/reference numbers appended.

## Change
- Add suffix: EFTPOS Purchase/Cash Out receipt details
- Add suffix: "Last 4 Card Digits XXXX" trailing pattern
- Add prefix: "Visa Debit Purchase Card XXXX"
- Add suffix: "Foreign Currency Amount:.*In \\d+ Date.*"
