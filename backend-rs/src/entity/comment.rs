use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_comment")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub memo_id: i32,
    pub content: String,
    pub user_id: i32,
    pub user_name: String,
    pub mentioned: Option<String>,
    pub created: Option<DateTimeUtc>,
    pub updated: Option<DateTimeUtc>,
    pub mentioned_user_id: Option<String>,
    pub email: Option<String>,
    pub link: Option<String>,
    pub approved: Option<i32>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
