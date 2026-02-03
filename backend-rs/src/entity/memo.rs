use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "t_memo")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub user_id: i32,
    pub content: Option<String>,
    pub tags: Option<String>,
    pub visibility: Option<String>,
    pub status: Option<String>,
    pub created: Option<DateTimeUtc>,
    pub updated: Option<DateTimeUtc>,
    pub priority: Option<i32>,
    pub comment_count: Option<i32>,
    pub like_count: Option<i32>,
    pub enable_comment: Option<i32>,
    pub view_count: Option<i32>,
    pub source: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
