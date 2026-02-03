package st.coo.memo.entity;

import com.mybatisflex.annotation.Id;
import com.mybatisflex.annotation.KeyType;
import com.mybatisflex.annotation.Table;
import lombok.Getter;
import lombok.Setter;

import java.io.Serializable;
import java.sql.Timestamp;


@Setter
@Getter
@Table(value = "t_user_memo_relation")
public class TUserMemoRelation implements Serializable {

    
    @Id(keyType = KeyType.Auto)
    private Integer id;

    
    private Integer memoId;

    
    private Integer userId;

    
    private String favType;

    
    private Timestamp created;

    
    private Timestamp updated;

}
