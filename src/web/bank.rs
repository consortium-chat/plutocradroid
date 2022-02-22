use chrono::TimeZone;

use crate::models::TransferType;
use crate::models;
use super::prelude::*;
use super::motions;


#[derive(Debug, Clone)]
pub struct GiveDestination {
    expected_name: Option<String>,
    id: models::UserId,
}

use rocket::http::RawStr;
use std::str::FromStr;
impl<'v> rocket::request::FromFormValue<'v> for GiveDestination {
    type Error = &'v RawStr;

    fn from_form_value(form_value: &'v RawStr) -> Result<Self, Self::Error> {
        let s = <String as rocket::request::FromFormValue>::from_form_value(form_value)?;
        dbg!(&s);
        let c = match crate::GIVE_DESTINATION_RE.captures(s.as_str()) {
            Some(v) => v,
            None => return Err(form_value),
        };
        dbg!(&c);
        let expected_name = c.get(1).map(|v| v.as_str().to_string());
        let id_str = c.get(2).ok_or(form_value)?;
        let id:models::UserId = i64::from_str(id_str.as_str()).map_err(|_| form_value)?.try_into().map_err(|_| form_value)?;
        dbg!();
        Ok(Self{expected_name, id})
    }
}

#[derive(Debug, Clone, FromForm)]
pub struct GiveForm {
    csrf: String,
    quantity: i64,
    ty: String,
    destination: GiveDestination,
}

sql_function!{
    #[sql_name = "coalesce"]
    fn coalesce_2<T: diesel::sql_types::NotNull>(a: diesel::sql_types::Nullable<T>, b: T) -> T;
}

#[get("/my-transactions?<before_ms>&<fun_ty>")]
pub fn my_transactions(
    mut ctx: CommonContext,
    fun_ty: Option<String>,
    before_ms: Option<i64>,
) -> PlutoResponse {
    use crate::view_schema::balance_history::dsl as bh;
    use crate::schema::item_types::dsl as it;
    let before_ms = before_ms.unwrap_or(i64::MAX);
    #[cfg(feature = "debug")]
    let limit = 10;
    #[cfg(not(feature = "debug"))]
    let limit = 1000;
    let fun_ty_string = fun_ty.unwrap_or_else(|| String::from("all"));
    #[derive(Debug,Clone,PartialEq,Eq)]
    enum FungibleSelection {
        All,
        Specific(String),
    }
    
    impl FungibleSelection {
        pub fn as_str(&self) -> &str {
            match self {
                FungibleSelection::All => "all",
                FungibleSelection::Specific(s) => s,
            }
        }

        pub fn as_option(&self) -> Option<&str> {
            match self {
                FungibleSelection::All => None,
                FungibleSelection::Specific(s) => Some(s.as_str()),
            }
        }
    }
    #[derive(Debug,Clone,Queryable)]
    struct Transaction {
        //pub rowid:i64,
        pub balance:i64,
        pub quantity:i64,
        pub sign:i32,
        pub happened_at:DateTime<Utc>,
        pub ty:String,
        pub comment:Option<String>,
        pub other_party:Option<i64>,
        pub to_motion:Option<i64>,
        pub to_votes:Option<i64>,
        //pub message_id:Option<i64>,
        pub transfer_ty:TransferType,
        pub auction_id:Option<i64>,
    }
    let transaction_cols = (
        //bh::rowid,
        bh::balance,
        bh::quantity,
        bh::sign,
        bh::happened_at,
        bh::ty,
        bh::comment,
        bh::other_party,
        bh::to_motion,
        bh::to_votes,
        //bh::message_id,
        bh::transfer_ty,
        bh::auction_id,
    );
    #[derive(Debug,Clone)]
    enum TransactionView {
        Generated{amt: i64, bal: i64},
        Trans(Transaction),
    }
    let fun_tys:Vec<String> = it::item_types.select(it::name).order(it::position).get_results(&*ctx).unwrap();
    let fun_ty = if fun_ty_string == "all" {
        FungibleSelection::All
    } else if fun_tys.iter().any(|ft| ft.as_str() == fun_ty_string) {
        FungibleSelection::Specific(fun_ty_string)
    } else {
        return hard_err(Status::BadRequest)
    };
    let txns:Option<(Vec<_>,bool)> = ctx.deets.as_ref().map(|deets| {
        let q = bh::balance_history
            .select(transaction_cols)
            .filter(bh::user.eq(deets.id()))
            .filter(coalesce_2(bh::ty.nullable().eq(fun_ty.as_option()).nullable(), true))
            .filter(coalesce_2(bh::happened_at.nullable().lt(Utc.timestamp_millis_opt(before_ms).single()).nullable(),true))
            .filter(bh::transfer_ty.ne(TransferType::Generated))
            .order((bh::happened_at.desc(), bh::rowid.desc(), bh::sign.desc()))
            .limit(limit+1);
        info!("{}", diesel::debug_query(&q));
        let txns:Vec<Transaction> = q.get_results(&*ctx)
            .unwrap();
        info!("{} txns results", txns.len());
        let mut gen_txns:Vec<Transaction> = if let [.., last] = txns.as_slice() {
            bh::balance_history
                .select(transaction_cols)
                .filter(bh::user.eq(deets.id()))
                .filter(coalesce_2(bh::ty.nullable().eq(fun_ty.as_option()).nullable(), true))
                .filter(coalesce_2(bh::happened_at.nullable().lt(Utc.timestamp_millis_opt(before_ms).single()).nullable(),true))
                .filter(bh::happened_at.gt(last.happened_at))
                .filter(bh::transfer_ty.eq(TransferType::Generated))
                .order((bh::happened_at.desc(), bh::rowid.desc()))
                .get_results(&*ctx)
                .unwrap()
        } else { Vec::new() };
        let mut txn_views = Vec::new();
        let (hit_limit,iter) = if txns.len() == ((limit+1) as usize) {
            (true, txns[..txns.len()-1].iter())
        } else { (false, txns.iter()) };
        for txn in iter.rev() {
            let mut amt = 0;
            let mut bal = 0;
            while gen_txns.last().map(|t| t.happened_at < txn.happened_at).unwrap_or(false) {
                let gen_txn = gen_txns.pop().unwrap();
                amt += gen_txn.quantity;
                bal = gen_txn.balance;
            }
            if amt > 0 {
                txn_views.push(TransactionView::Generated{amt, bal});
            }
            txn_views.push(TransactionView::Trans(txn.clone()));
        }
        let mut amt = 0;
        let mut bal = 0;
        while let Some(gt) = gen_txns.pop() {
            amt += gt.quantity;
            bal = gt.balance;
        }
        if amt > 0 {
            txn_views.push(TransactionView::Generated{amt,bal});
        }
        txn_views.reverse();
        (txn_views, hit_limit)
    });
    let main_body = html!{
        main {
            h1 { "My Transactions" }
            @if let Some((txns, hit_limit)) = txns {
                form.tall-form {
                    div { "Show transactions in:" }
                    @for ft in &fun_tys {
                        label {
                            input type="radio" name="fun_ty" value=(ft) checked?[fun_ty == FungibleSelection::Specific(ft.clone())];
                            (ft)
                        }
                    }
                    label {
                        input type="radio" name="fun_ty" value="all" checked?[fun_ty == FungibleSelection::All];
                        "All currencies"
                    }
                    .spacer-tall {}
                    button type="submit" { "Go" }
                    .spacer-tall {}
                }
                table.tabley-table {
                    thead {
                        tr {
                            th { "Timestamp" }
                            th { "Description" }
                            th { "Amount" }
                            th { "Running Total" }
                        }
                    }
                    tbody {
                        @for txn_view in &txns {
                            @if let TransactionView::Trans(txn) = txn_view {
                                tr.transaction {
                                    td {
                                        (show_ts(txn.happened_at))
                                    }
                                    td {
                                        @match txn.transfer_ty {
                                            TransferType::Give | TransferType::AdminGive => {
                                                @if txn.transfer_ty == TransferType::AdminGive {
                                                    "admin "
                                                }
                                                @if txn.sign < 0 {
                                                    "transfer to "
                                                } @else {
                                                    "transfer from "
                                                }
                                                "user#\u{200B}"
                                                (txn.other_party.unwrap())
                                            },
                                            TransferType::MotionCreate => {
                                                @let damm_id = crate::damm::add_to_str(txn.to_motion.unwrap().to_string());
                                                "1 vote, created "
                                                a href=(uri!(motions::motion_view:damm_id = &damm_id, cb = _)) {
                                                    "motion #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::MotionVote => {
                                                @let motion_id = &txn.to_motion.unwrap();
                                                @let votes = &txn.to_votes.unwrap();
                                                @let damm_id = crate::damm::add_to_str(motion_id.to_string());
                                                (votes)
                                                " vote(s) on "
                                                a href=(uri!(motions::motion_view:damm_id = &damm_id, cb = _)) {
                                                    "motion #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AdminFabricate | TransferType::CommandFabricate => {
                                                "fabrication"
                                            },
                                            TransferType::AuctionCreate => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Created "
                                                a href=(uri!(motions::motion_view:damm_id = &damm_id, cb = _)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AuctionReserve => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Bid on "
                                                a href=(uri!(motions::motion_view:damm_id = &damm_id, cb = _)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AuctionRefund => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Outbid on "
                                                a href=(uri!(super::auctions::auction_view:damm_id = &damm_id, cb = _)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::AuctionPayout => {
                                                @let damm_id = crate::damm::add_to_str(txn.auction_id.unwrap().to_string());
                                                "Won the bid, payout for "
                                                a href=(uri!(super::auctions::auction_view:damm_id = &damm_id, cb = _)) {
                                                    "auction #"
                                                    (&damm_id)
                                                }
                                            },
                                            TransferType::Generated => "unreachable",
                                        }
                                        @if let Some(comment) = &txn.comment {
                                            " “" (comment) "”"
                                        }
                                    }
                                    td.amount.negative[txn.sign < 0] {
                                        span.paren { "(" }
                                        span.amount-inner { (txn.quantity) }
                                        span.ty { (txn.ty) }
                                        span.paren { ")" }
                                    }
                                    td.running-total {
                                        span.amount-inner { (txn.balance) }
                                        span.ty { (txn.ty) }
                                    }
                                }
                            } @else {
                                @let (amt, bal) = match txn_view { TransactionView::Generated{amt, bal} => (amt, bal), _ => unreachable!() };
                                tr.transaction.generated {
                                    td {}
                                    td { "generator outputs" }
                                    td.amount {
                                        span.paren { "(" }
                                        span.amount-inner { (amt) }
                                        span.ty { "pc" }
                                        span.paren { ")" }
                                    }
                                    td.running-total {
                                        span.amount-inner { (bal) }
                                        span.ty { "pc" }
                                    }
                                }
                            }
                        }
                        @if txns.is_empty() {
                            tr {
                                td colspan="4" {
                                    "Nothing to show."
                                }
                            }
                        }
                    }
                }
                @if hit_limit {
                    @let txn = match txns.iter().rev().find(|t| matches!(t, TransactionView::Trans(_))) { Some(TransactionView::Trans(t)) => t, d => {dbg!(d);unreachable!()} };
                    a href=(uri!(my_transactions: before_ms = txn.happened_at.timestamp_millis(), fun_ty = fun_ty.as_str())) { "Next" }
                }
            } @else {
                p { "You must be logged in to view your transactions." }
            }
        }
    };

    page(
        &mut ctx,
        PageTitle("My Transactions".to_string()),
        CanonicalUrl(None),
        html!{},
        main_body,
    )
}

#[get("/give")]
pub fn give_form(
    mut ctx: CommonContext
) -> PlutoResponse {
    if ctx.deets.is_none() {
        return soft_err("You're not logged in; Please log in to transfer fungibles.");
    }

    use schema::item_types::dsl as itdsl;

    let item_types:Vec<models::ItemType> = itdsl::item_types
        .select(models::ItemType::cols())
        .order(itdsl::position)
        .get_results(&*ctx)
        .unwrap();

    let csrf = ctx.csrf_token.clone();

    let body = html!{
        datalist id="known_users" {
            @for (discord_id,name) in crate::names::KNOWN_NAMES.entries() {
                option {
                    (name) " - " (discord_id)
                }
            }
        }
        form action="/give" method="post" {
            input type="hidden" name="csrf" value=(csrf);
            "Give"
            br;
            input type="number" min="1" name="quantity" placeholder="2331";
            br;
            select name="ty" {
                @for it in item_types {
                    option value=(it.id) { (it.long_name_ambiguous) }
                }
            }
            br;
            "to"
            br;
            input name="destination" type="text" list="known_users" pattern="(\\w+\\s*-\\s*)?\\d+" id="transfer_destination_input";
            br;
            "Type a discord snowflake id here, or start typing a name and select from the list. "
            b { "Be careful" } 
            "; When given without a name, this has no way to verify an id is valid, and will transfer to any id even if it doesn't exist."
            br;
            br;
            button type="submit" {
                "Send (no backsies)"
            }
        }
    };

    page(
        &mut ctx,
        PageTitle("Send Fungibles"),
        full_url(uri!(give_form)).into(),
        html!{},
        body,
    )
}

#[post("/give", data = "<data>")]
pub fn give_perform(
    mut ctx: CommonContext,
    data: LenientForm<GiveForm>,
) -> PlutoResponse {
    let now = Utc::now();

    if ctx.cookies.get(CSRF_COOKIE_NAME).map(|token| token.value()) != Some(data.csrf.as_str()) {
        return hard_err(rocket::http::Status::BadRequest);
    }

    if data.quantity < 0 {
        return hard_err(rocket::http::Status::BadRequest);
    }

    let deets = if let Some(ref d) = ctx.deets { d } else { return hard_err(rocket::http::Status::BadRequest); };

    use schema::item_types::dsl as itdsl;
    let maybe_ty:Option<models::ItemType> = itdsl::item_types
        .select(models::ItemType::cols())
        .filter(itdsl::name.eq(data.ty.as_str()))
        .get_result(&*ctx)
        .optional()
        .unwrap();
    
    let ty = match maybe_ty {
        Some(v) => v,
        None => return hard_err(rocket::http::Status::BadRequest),
    };

    if let Some(ref name) = data.destination.expected_name {
        let maybe_known_name = crate::names::KNOWN_NAMES.get(&(data.destination.id.into_u64()));
        if let Some(known_name) = maybe_known_name {
            if *known_name != name.as_str() {
                return soft_err(format!(
                    r#"Failed: The name "{name}" does not match the name on record, "{known_name}"."#
                ));
            } // else it matches, all is well
        } else {
            return soft_err(format!(
                r#"Failed: The name "{name}" is not known."#
            ));
        }
    }

    let t = TransactionBuilder::new(
        data.quantity,
        ty.id,
        now
    ).give( 
        deets.id(),
        data.destination.id,
        false
    );

    let mut res = None;
    ctx.conn.transaction::<_,diesel::result::Error,_>(|| {
        match TransferHandler::handle_single(&*ctx, t) {
            Err(TransferError::NotEnough) => {
                res = Some(
                    soft_err("Failed: You do not have enough")
                );
            },
            Err(TransferError::Overflow) => {
                res = Some(
                    soft_err("Failed: Overflow")
                )
            },
            Ok(v) => v.unwrap(),
        }

        Ok(())
    }).unwrap();

    if let Some(res) = res { return res }

    page(
        &mut ctx,
        PageTitle("Successfully transferred"),
        CanonicalUrl(None),
        html!{},
        html!{
            "Success: Transferred "
            (data.quantity)
            " "
            (ty.long_name_ambiguous)
            " to "
            @if let Some(ref name) = data.destination.expected_name {
                (name) " (id " (data.destination.id) ")"
            } @else {
                "id " (data.destination.id)
            }
            "."
            br;
            br;
            a href="/give" { "Transfer some more" }
        }
    )
}