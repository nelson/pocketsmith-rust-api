# Iteration 024: Final cluster cleanup

**Changes**:
- stage4: CommInsure banking_identity_mapping updated for XX prefix
- stage4: International Transaction Fee as merchant_mapping (was missing — typed as merchant not banking_operation)
- stage4: Papparich Macquarie consolidation
- stage4: ATM Burwood terminal letter stripping (goes through banking path, not merchant)
- stage4: Jason Hu → Jason Hui truncation
- stage4: Airport Retail Enterprises truncation
- stage4: McDonald's Dt (drive-through) + Mcdonald's variant
- stage4: Burwood Discount Chemist with/without location
- stage4: PayPal BudgetPetProducts, Cash Deposit Beem It, Saidgani Musaev, Mrs Stephanie Wong Deposit, Joint Account Deposit — receipt/memo stripping
- stage4: Added Saidgani Musaev + Tam S & Tam L N to persons_strip_memo

**Result**: Q +0.04 (93.19→93.23). 18 unique payees eliminated, 17 fewer long-tail, 12 fewer singletons.
