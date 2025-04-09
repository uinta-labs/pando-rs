use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "schedule")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        default = "uuid_generate_v4()"
    )]
    pub id: uuid::Uuid,

    #[sea_orm(column_type = "JsonBinary")]
    pub body: Json,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::device_schedule::Entity")]
    DeviceSchedule,
}

impl Related<super::device_schedule::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DeviceSchedule.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
