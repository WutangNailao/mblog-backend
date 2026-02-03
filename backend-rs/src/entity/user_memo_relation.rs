use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_user_memo_relation")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub memo_id: i32,
    pub user_id: i32,
    pub fav_type: String,
    pub created: Option<DateTimeUtc>,
    pub updated: Option<DateTimeUtc>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
