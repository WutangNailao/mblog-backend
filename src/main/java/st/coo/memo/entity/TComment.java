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
@Table(value = "t_comment")
public class TComment implements Serializable {

    
    @Id(keyType = KeyType.Auto)
    private Integer id;

    
    private Integer memoId;

    
    private String content;

    
    private Integer userId;

    
    private String userName;

    
    private String mentioned;

    
    private Timestamp created;

    
    private Timestamp updated;

    
    private String mentionedUserId;

    
    private String email;

    
    private String link;

    
    private Integer approved;

}
