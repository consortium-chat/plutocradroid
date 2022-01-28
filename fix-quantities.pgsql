-- as dictated in motion#2410, this should give a list of fungibles that need to be ""fabricated"" to make the transaction history consistent
select
    sign,
    ty,
    sum(quantity)
from
    (select 
        lag(b.rowid,1,-1) over (partition by b."user", b.ty order by b.happened_at desc, b.rowid desc) as prev_rowid,
        b.rowid, b.sign,
        b."user",
        b.ty,
        b.transfer_ty,
        lag(b.balance,1,0) over (partition by b."user", b.ty order by b.happened_at desc, b.rowid desc) as prev_balance,
        b.quantity,
        b.balance
        -- ((pb.rowid = b.rowid and pb.balance = b.balance) or (pb.balance + (b.quantity*b.sign) = b.balance)) as good
    from
        balance_history b) t
where
    not ((prev_rowid = rowid and prev_balance = balance) or (prev_balance + (quantity * sign) = balance))
group by
    sign,
    ty
;
