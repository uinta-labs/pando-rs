use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize)]
#[sea_orm(table_name = "device")]
pub struct Model {
    #[sea_orm(
        primary_key,
        auto_increment = false,
        default = "uuid_generate_v4()"
    )]
    pub id: uuid::Uuid,

    #[sea_orm(column_type = "Uuid")]
    pub fleet_id: uuid::Uuid,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        has_many = "super::device_schedule::Entity",
        from = "Column::Id",
        to = "super::device_schedule::Column::DeviceId"
    )]
    DeviceSchedule,

    #[sea_orm(
        has_one = "super::waiting_room::Entity"
    )]
    WaitingRoom,

    #[sea_orm(
        belongs_to = "super::fleet::Entity",
        from = "Column::FleetId",
        to = "super::fleet::Column::Id"
    )]
    Fleet,
}

impl Related<super::device_schedule::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::DeviceSchedule.def()
    }
}

impl Related<super::waiting_room::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::WaitingRoom.def()
    }
}

impl Related<super::fleet::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Fleet.def()
    }
}

// impl ColumnTrait for Column {
//     type EntityName = Entity;

//     fn def(&self) -> ColumnDef {
//         match self {
//             Column::Id => ColumnType::Uuid
//                 .def()
//                 .indexed()
//                 .default("uuid_generate_v4()"),
//         }
//     }
// }

impl ActiveModelBehavior for ActiveModel {}
